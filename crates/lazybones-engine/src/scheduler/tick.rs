//! One scheduler pass: reconcile → promote → claim → spawn.
//!
//! The supervisor loop calls this every `tick_secs`. Claimed tasks are handed to
//! [`finish::drive`] in their own tokio task (step 4–5), so the tick itself never
//! blocks on agent work.

use lazybones_store::{Status, StoreHandle, Transition};

use crate::config::EngineConfig;
use crate::hcom::Hcom;

use super::{finish, prompt, reclaim, worktree};

/// The actor recorded on transitions the tick drives.
const ACTOR: &str = "scheduler:tick";

/// Run one full pass. Best-effort throughout: a single task's failure is logged
/// and never aborts the pass.
pub async fn tick(store: &StoreHandle, hcom: &Hcom, cfg: &EngineConfig) {
    reclaim::reconcile(store, hcom, cfg).await;
    promote(store).await;
    claim_and_spawn(store, hcom, cfg).await;
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

    for task in ready.into_iter().take(budget) {
        // 1. Provision BEFORE claim so a failure blocks cleanly (no half-claim).
        let provisioned = match worktree::provision(&task, cfg).await {
            Ok(p) => p,
            Err(e) => {
                block(store, &task.id, format!("worktree provisioning failed: {e}")).await;
                continue;
            }
        };

        // 2. Spawn the agent, capturing the hcom name.
        let tool = task.tool.clone().unwrap_or_else(|| cfg.agent_tool.clone());
        let agent_prompt = prompt::compose(
            &task,
            &provisioned.worktree,
            &provisioned.branch,
            &cfg.remote,
        );
        let session = match hcom
            .spawn(
                &tool,
                &task.id,
                std::path::Path::new(&provisioned.worktree),
                &agent_prompt,
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
            claimed,
        ));
    }
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
