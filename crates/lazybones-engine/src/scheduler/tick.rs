//! One scheduler pass: reconcile → promote → claim → spawn → tail.
//!
//! The supervisor loop calls this every `tick_secs`. Claimed tasks are handed to
//! [`finish::drive`] in their own tokio task (step 4–5), so the tick itself never
//! blocks on agent work.

use std::collections::HashMap;

use lazybones_store::{Lifecycle, Run, Status, StoreHandle, Task, Transition};

use crate::config::EngineConfig;
use crate::hcom::Hcom;

use super::block::block;
use super::effective::{self, EffectiveGit};
use super::{auto_pr, finish, hcom_tail, issue, prompt, reclaim, worktree};

/// The actor recorded on transitions the tick drives.
const ACTOR: &str = "scheduler:tick";

/// Run one full pass. Best-effort throughout: a single task's failure is logged
/// and never aborts the pass.
///
/// `tick_count` is the monotonic pass counter the supervisor loop threads in; it
/// gates the coarse-cadence reverse issue→task sync (every Nth tick).
pub async fn tick(store: &StoreHandle, hcom: &Hcom, cfg: &EngineConfig, tick_count: u64) {
    reclaim::reconcile(store, hcom, cfg).await;
    promote(store).await;
    claim_and_spawn(store, hcom, cfg).await;
    // AUTO-PR: for any opt-in workflow whose every task is now done and has no PR
    // yet, spawn the configured agent to summarize and `gh pr create`. Best-effort,
    // idempotent (guarded by `run.pr_url`); never blocks claim/spawn.
    auto_pr::open_prs_for_completed_runs(store, hcom, cfg).await;
    // TAIL: drain hcom's raw event stream into the durable hcom log. Best-effort,
    // self-contained, holds no await on agent work (docs/hcom-logs-scope.md).
    hcom_tail::tail_hcom(store, hcom).await;
    // REVERSE ISSUE SYNC: pull linked GitHub issues' state back onto tasks. Runs
    // after the tail so it never blocks claim/spawn, and only every Nth tick so
    // the extra `gh` calls stay cheap (`0` disables it). Best-effort.
    if reverse_sync_due(cfg.issue_sync_every_n_ticks, tick_count) {
        issue::reverse_sync(store, &lazybones_gh::Gh::new()).await;
    }
}

/// Whether the reverse issue-sync should run on this tick. `every == 0` disables
/// it; otherwise it runs once every `every` ticks (and on the very first tick so
/// a fresh daemon syncs promptly rather than waiting a full window).
fn reverse_sync_due(every: u64, tick_count: u64) -> bool {
    every != 0 && tick_count.is_multiple_of(every)
}

/// PROMOTE: every `pending` task whose deps are all `done` → `ready`, except
/// tasks in a stopped (paused) workflow — a stopped run promotes nothing.
async fn promote(store: &StoreHandle) {
    // The paused workflows to exclude. On a query failure, fall back to "none
    // stopped" — promoting is the safe default; the claim guard still refuses to
    // spawn for a stopped run, so nothing actually runs.
    let stopped = store.unpromotable_run_ids().await.unwrap_or_default();
    let ready = match store.newly_ready(&stopped).await {
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

    // `Shared`-mode serialization guard. A shared run has ONE worktree+branch for
    // all its tasks, so two agents must never work it at once. Seed the set with
    // run_ids that already have a running task, and add each run we claim into
    // this tick; a Shared task whose run is in the set is held back (stays ready,
    // claimed a later tick once its sibling finishes). Non-Shared runs never enter
    // the set, so this is inert for the isolated default.
    let mut busy_shared_runs: std::collections::HashSet<String> = store
        .list_tasks(Some(Status::Running))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| t.run_id)
        .collect();

    for task in ordered.into_iter().take(budget) {
        let run = run_for(store, &task, &mut runs).await;

        // A workflow that isn't actively started claims nothing: skip its tasks
        // even if they are `ready` (a human reset/reclaim can leave ready tasks
        // behind). This is the spawn-side guard that closes the "cancelled run
        // still runs" bug — a revived task in a stopped run must never be
        // re-claimed — and also the "created-but-not-started run runs" bug: an
        // `active` run with no `started_at` has not been started by an operator.
        // Standalone tasks (no parent run) are unaffected.
        if run
            .as_ref()
            .is_some_and(|r| r.lifecycle != Lifecycle::Active || r.started_at.is_none())
        {
            continue;
        }

        let eff = effective::resolve(&task, run.as_ref(), cfg);

        // Shared-mode serialization: if another task of this run already holds the
        // shared tree (running, or claimed earlier this tick), hold this one back —
        // it stays `ready` and is picked up once the sibling lands. Only Shared
        // tasks with a real run consult this; everything else falls straight
        // through. The budget slot isn't consumed (we `continue` before claim), so
        // sibling runs still fill it.
        if eff.worktree_mode == lazybones_store::WorktreeMode::Shared
            && let Some(run_id) = task.run_id.as_deref()
            && busy_shared_runs.contains(run_id)
        {
            continue;
        }

        // Resolve a `reuse_from` source worktree before provisioning (the
        // scheduler reads the source task from the store — no HTTP to itself).
        let reuse_path = match resolve_reuse(store, &task, &eff).await {
            Ok(p) => p,
            Err(reason) => {
                block(store, &task, reason, ACTOR).await;
                continue;
            }
        };

        // 1. Provision BEFORE claim so a failure blocks cleanly (no half-claim).
        let provisioned = match worktree::provision(&task, &eff, cfg, reuse_path.as_deref()).await {
            Ok(p) => p,
            Err(e) => {
                block(
                    store,
                    &task,
                    format!("worktree provisioning failed: {e}"),
                    ACTOR,
                )
                .await;
                continue;
            }
        };

        // 2. Spawn the agent, capturing the hcom name. The agent triple is
        //    resolved task ?? workspace ?? global by `effective::resolve` above.
        //    Any prior operator conversation is folded into the prompt so a
        //    revived task resumes with the operator's guidance (empty on a first
        //    claim); a read failure is non-fatal — we just spawn without it.
        let history = store.chat_history(&task.id).await.unwrap_or_default();
        let agent_prompt = prompt::compose(
            &task,
            &provisioned.worktree,
            &provisioned.branch,
            &cfg.remote,
            &history,
        );
        // Gate-bypass flags for this tool (empty if none configured); without
        // them a headless claude in a fresh worktree stalls on its trust prompt.
        let perm_flags = cfg
            .permission_flags
            .get(&eff.tool)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let session = match hcom
            .spawn(
                &eff.tool,
                &task.id,
                std::path::Path::new(&provisioned.worktree),
                &agent_prompt,
                crate::hcom::AgentLaunch {
                    model: eff.model.as_deref(),
                    effort: eff.effort.as_deref(),
                    permission_flags: perm_flags,
                },
            )
            .await
        {
            Ok(name) => name,
            Err(e) => {
                block(store, &task, format!("agent spawn failed: {e}"), ACTOR).await;
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

        // Mark this run's shared tree as taken for the rest of the tick, so a
        // sibling Shared task ordered after us waits its turn. Harmless for
        // non-Shared runs (their tasks never consult the set).
        if let Some(run_id) = claimed.run_id.clone() {
            busy_shared_runs.insert(run_id);
        }

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
