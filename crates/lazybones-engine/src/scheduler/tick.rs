//! One scheduler pass: reconcile → promote → claim → spawn → tail.
//!
//! The supervisor loop calls this every `tick_secs`. Claimed tasks are handed to
//! [`finish::drive`] in their own tokio task (step 4–5), so the tick itself never
//! blocks on agent work.

use std::collections::HashMap;

use lazybones_store::{Run, Status, StoreHandle, Task, Transition};

use crate::config::EngineConfig;
use crate::hcom::Hcom;

use super::effective::{self, EffectiveGit};
use super::{finish, hcom_tail, prompt, reclaim, worktree};

/// The actor recorded on transitions the tick drives.
const ACTOR: &str = "scheduler:tick";

/// Run one full pass. Best-effort throughout: a single task's failure is logged
/// and never aborts the pass.
pub async fn tick(store: &StoreHandle, hcom: &Hcom, cfg: &EngineConfig) {
    reclaim::reconcile(store, hcom, cfg).await;
    promote(store).await;
    claim_and_spawn(store, hcom, cfg).await;
    // TAIL: drain hcom's raw event stream into the durable hcom log. Best-effort,
    // self-contained, holds no await on agent work (docs/hcom-logs-scope.md).
    hcom_tail::tail_hcom(store, hcom).await;
}

/// PROMOTE: every `pending` task whose deps are all `done` → `ready`.
async fn promote(store: &StoreHandle) {
    let ready = match store.newly_ready().await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!("promote: newly_ready failed: {e}");
            return;
        }
    };
    for id in ready {
        match store.transition(&id, Transition::Ready, ACTOR).await {
            Ok(_) => tracing::info!(task = %id, "promoted to ready"),
            Err(e) => tracing::warn!(task = %id, "promote transition failed: {e}"),
        }
    }
}

/// CLAIM: provision + claim + spawn up to the remaining concurrency budget.
async fn claim_and_spawn(store: &StoreHandle, hcom: &Hcom, cfg: &EngineConfig) {
    let budget = match budget(store, cfg).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("claim: budget query failed: {e}");
            return;
        }
    };
    if budget == 0 {
        return;
    }

    let ready = match store.list_tasks(Some(Status::Ready)).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("claim: list ready failed: {e}");
            return;
        }
    };

    // Per-run fairness: when more tasks are ready than the budget allows,
    // round-robin across distinct `run_id`s so one big workflow can't starve the
    // others. Standalone tasks (run_id = None) share a single bucket.
    let ordered = fair_order(ready);

    // Cache the parent Run per run_id so we read each workflow at most once a tick.
    let mut runs: HashMap<String, Option<Run>> = HashMap::new();

    for task in ordered.into_iter().take(budget) {
        let run = run_for(store, &task, &mut runs).await;
        let eff = effective::resolve(&task, run.as_ref(), cfg);

        // Resolve a `reuse_from` source worktree before provisioning (the
        // scheduler reads the source task from the store — no HTTP to itself).
        let reuse_path = match resolve_reuse(store, &task, &eff).await {
            Ok(p) => p,
            Err(reason) => {
                block(store, &task.id, reason).await;
                continue;
            }
        };

        // 1. Provision BEFORE claim so a failure blocks cleanly (no half-claim).
        let provisioned = match worktree::provision(&task, &eff, cfg, reuse_path.as_deref()).await {
            Ok(p) => p,
            Err(e) => {
                block(store, &task.id, format!("worktree provisioning failed: {e}")).await;
                continue;
            }
        };

        // 2. Spawn the agent, capturing the hcom name. The agent triple is
        //    resolved task ?? workspace ?? global by `effective::resolve` above.
        let agent_prompt = prompt::compose(
            &task,
            &provisioned.worktree,
            &provisioned.branch,
            &cfg.remote,
        );
        let session = match hcom
            .spawn(
                &eff.tool,
                &task.id,
                std::path::Path::new(&provisioned.worktree),
                &agent_prompt,
                eff.model.as_deref(),
                eff.effort.as_deref(),
            )
            .await
        {
            Ok(name) => name,
            Err(e) => {
                block(store, &task.id, format!("agent spawn failed: {e}")).await;
                continue;
            }
        };

        // 3. Claim: ready → running, recording the session + worktree + branch.
        let claimed = store
            .transition(
                &task.id,
                Transition::Claim {
                    session,
                    worktree: provisioned.worktree.clone(),
                    branch: provisioned.branch.clone(),
                },
                ACTOR,
            )
            .await;
        let claimed = match claimed {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(task = %task.id, "claim transition failed: {e}");
                continue;
            }
        };
        tracing::info!(task = %claimed.id, "claimed and spawned agent");

        // 4–5. Drive await → gate → finish in its own task.
        tokio::spawn(finish::drive(
            store.clone(),
            hcom.clone(),
            cfg.clone(),
            eff,
            claimed,
        ));
    }
}

/// Read (and cache) the parent [`Run`] for `task`, if it has a `run_id`.
///
/// A standalone task (no `run_id`) resolves to `None` and behaves exactly as
/// before. A workflow task whose run row is somehow missing also resolves to
/// `None` (the effective settings fall back to the global config).
async fn run_for(
    store: &StoreHandle,
    task: &Task,
    cache: &mut HashMap<String, Option<Run>>,
) -> Option<Run> {
    let run_id = task.run_id.as_ref()?;
    if let Some(cached) = cache.get(run_id) {
        return cached.clone();
    }
    let run = match store.get_run(run_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(run = %run_id, "claim: get_run failed: {e}");
            None
        }
    };
    cache.insert(run_id.clone(), run.clone());
    run
}

/// Resolve the worktree a `reuse`-mode task should run in via its `reuse_from`
/// link: read that source task's stored `worktree`. Returns `Ok(None)` when no
/// resolution is needed (mode isn't reuse, or no `reuse_from` is set — the
/// task's own worktree is used by `provision`).
///
/// # Errors
/// Returns a block reason (a `String`) when `reuse_from` points at a task that
/// has no worktree yet (not claimed / torn down) or cannot be read.
async fn resolve_reuse(
    store: &StoreHandle,
    task: &Task,
    eff: &EffectiveGit,
) -> Result<Option<String>, String> {
    use lazybones_store::WorktreeMode;
    if eff.worktree_mode != WorktreeMode::Reuse {
        return Ok(None);
    }
    let Some(source_id) = task.reuse_from.as_ref() else {
        return Ok(None);
    };
    match store.get_task(source_id).await {
        Ok(Some(source)) => source.worktree.map(Some).ok_or_else(|| {
            format!("reuse_from task `{source_id}` has no worktree yet (not claimed / torn down)")
        }),
        Ok(None) => Err(format!("reuse_from task `{source_id}` does not exist")),
        Err(e) => Err(format!("reuse_from lookup of `{source_id}` failed: {e}")),
    }
}

/// Order ready tasks for fair claiming: round-robin across distinct `run_id`s so
/// no single workflow starves the others when ready > budget. Within a run, list
/// order is preserved; standalone tasks (`run_id = None`) share one bucket keyed
/// by the empty string. Deterministic for a given input.
fn fair_order(ready: Vec<Task>) -> Vec<Task> {
    // Preserve first-seen run order, then interleave one task per run per round.
    let mut order: Vec<String> = Vec::new();
    let mut buckets: HashMap<String, Vec<Task>> = HashMap::new();
    for task in ready {
        let key = task.run_id.clone().unwrap_or_default();
        if !buckets.contains_key(&key) {
            order.push(key.clone());
        }
        buckets.entry(key).or_default().push(task);
    }

    let mut out = Vec::new();
    let mut idx = 0;
    let mut drained = false;
    while !drained {
        drained = true;
        for key in &order {
            let bucket = buckets.get_mut(key).expect("bucket exists for ordered key");
            if let Some(task) = bucket.get(idx).cloned() {
                out.push(task);
                drained = false;
            }
        }
        idx += 1;
    }
    out
}

/// The remaining concurrency budget = `concurrency - count(running, gating)`.
async fn budget(store: &StoreHandle, cfg: &EngineConfig) -> anyhow::Result<usize> {
    let running = store.list_tasks(Some(Status::Running)).await?.len();
    let gating = store.list_tasks(Some(Status::Gating)).await?.len();
    Ok(cfg.concurrency.saturating_sub(running + gating))
}

/// Block a task with `reason`, logging any failure.
async fn block(store: &StoreHandle, id: &str, reason: String) {
    if let Err(e) = store
        .transition(id, Transition::Block { reason }, ACTOR)
        .await
    {
        tracing::warn!(task = %id, "block transition failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_in(id: &str, run_id: Option<&str>) -> Task {
        let mut t = Task::seed(id, "r", "T", "s", vec![], vec![], None);
        t.run_id = run_id.map(ToOwned::to_owned);
        t
    }

    #[test]
    fn fair_order_round_robins_across_runs() {
        // wf-1 has 3 ready, wf-2 has 1. Fair order interleaves so wf-2's single
        // task is reached early rather than after all of wf-1's.
        let ready = vec![
            task_in("a1", Some("wf-1")),
            task_in("a2", Some("wf-1")),
            task_in("a3", Some("wf-1")),
            task_in("b1", Some("wf-2")),
        ];
        let order: Vec<String> = fair_order(ready).into_iter().map(|t| t.id).collect();
        // Round 0: a1, b1; round 1: a2; round 2: a3.
        assert_eq!(order, vec!["a1", "b1", "a2", "a3"]);
    }

    #[test]
    fn fair_order_keeps_standalone_in_one_bucket() {
        let ready = vec![
            task_in("s1", None),
            task_in("w1", Some("wf-1")),
            task_in("s2", None),
        ];
        let order: Vec<String> = fair_order(ready).into_iter().map(|t| t.id).collect();
        // Standalone share a bucket (first-seen run order: standalone, then wf-1).
        // Round 0: s1, w1; round 1: s2.
        assert_eq!(order, vec!["s1", "w1", "s2"]);
    }
}
