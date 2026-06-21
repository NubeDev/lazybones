//! `POST /tasks/:id/retry` — revive ONE blocked task back into the pipeline.
//!
//! A blocked task is otherwise a dead-end: the scheduler's `promote()` only ever
//! revives `pending` tasks, so a timed-out/failed task can never re-enter the run
//! on its own. Retry is the targeted lever — it resets just this one task to
//! `pending` (clearing its claim/worktree/commit/heartbeat/reason via the shared
//! store [`reset`](lazybones_store::StoreHandle::reset)), so the next tick
//! auto-promotes it. No `start` is needed.
//!
//! Two shapes, chosen by whether the body carries a `strategy`:
//!
//! - **Guided** (`strategy` set) — the task failed for a *reason*, so re-running
//!   the same prompt unchanged would likely fail the same way. Revive it in its
//!   **kept worktree** (`blocked -> ready`) with the strategy's guidance folded
//!   into the re-spawn prompt (via the chat history `prompt::compose` reads), so
//!   the agent builds on its partial work. This is the operator counterpart to
//!   the hands-off auto-retry the scheduler runs.
//! - **Clean** (no `strategy`) — the transient-failure case. Mirror the per-task
//!   body of [`restart_workflow`](super::workflows_restart): kill any live agent,
//!   optionally tear the worktree down (`remove_worktrees`), then `reset` to a
//!   fresh `pending` (clearing the worktree and the auto-retry counter).
//!
//! Either way it is scoped to one id and refuses anything not actually stuck —
//! only a `blocked` task is revivable (a `done` task is finished work; restart it
//! to re-run). Requires `Block` (same authority as cancel). The next tick promotes
//! a reset task; a revived one is re-spawned in place.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{RetryStrategy, Status, StoreError, Task};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// Retry options.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RetryBody {
    /// The fix intent for a *guided* retry. When set, the task is revived in its
    /// kept worktree with the strategy's guidance folded into the re-spawn prompt
    /// (so the agent builds on its partial work). When `None`, the task is reset
    /// clean (the transient-failure case) — a fresh worktree, no guidance.
    pub strategy: Option<RetryStrategy>,
    /// Clean-reset only: also `git worktree remove --force` the task's tree before
    /// resetting. Ignored for a guided retry (which keeps the tree by design).
    pub remove_worktrees: bool,
}

/// Retry task `:id`. With a `strategy`: revive it in place with guidance. Without:
/// kill its live agent, optionally remove its worktree, reset it to `pending`.
/// `404` if the task is unknown; `409` if it isn't in a revivable (`blocked`)
/// state.
pub async fn retry_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    body: Option<Json<RetryBody>>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Block, "block", &id)?;
    let Json(opts) = body.unwrap_or_default();

    let task = state
        .store
        .get_task(&id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.clone()))?;

    // A stopped (paused) workflow's tasks are not revivable — resume it first, or
    // a revived task would silently re-run the moment the run resumes.
    super::guard::ensure_run_revivable(&state, &task).await?;

    // Only a stuck task is revivable here. A `done` task is finished, merged work
    // (restart the workflow to re-run it); a still-live task (pending/ready/
    // running/gating) is already in the pipeline and would be regressed.
    if task.status != Status::Blocked {
        return Err(ApiError::conflict(format!(
            "task `{id}` is `{}`, not blocked; only a blocked task can be retried",
            task.status.as_str()
        )));
    }

    // Guided retry: revive in the kept worktree with the strategy's guidance
    // folded into the re-spawn prompt, so the agent resumes on its partial work.
    // No kill / teardown — a blocked task has no live agent, and the tree is the
    // whole point. `bump_count = false`: a human-driven retry isn't capped (a
    // person is in the loop), unlike the scheduler's auto-retry.
    if let Some(strategy) = opts.strategy {
        let reason = task.reason.as_deref().unwrap_or("(no reason recorded)");
        let guidance = strategy.guidance(reason);
        let task = state
            .store
            .revive_with_guidance(&id, &guidance, session.actor(), false)
            .await?;
        return Ok(Json(task));
    }

    // Clean retry (transient failure): kill the live agent first so a claimed task
    // doesn't leave an orphan. A blocked task should have no agent, but a kill is
    // harmless and best-effort.
    if let Err(e) = lazybones_engine::cancel_agent(&id).await {
        tracing::warn!(task = %id, "retry: hcom kill failed (continuing): {e}");
    }

    // Optionally tear down the worktree (and its branch) before reset forgets
    // their values. The branch must go too, or the next New-mode claim's
    // `worktree add -b <branch>` collides with the leftover branch.
    if opts.remove_worktrees
        && let Some(path) = &task.worktree
    {
        // The workspace (and thus the repo to remove the tree from) hangs off the
        // parent workflow: prefer the `run_id` FK, falling back to the `run` label
        // for a standalone task.
        let run_key = task.run_id.as_deref().unwrap_or(&task.run);
        let run = state
            .store
            .get_run(run_key)
            .await?
            .ok_or_else(|| StoreError::RunNotFound(run_key.to_owned()))?;
        if let Err(e) = lazybones_engine::remove_worktree(
            std::path::Path::new(&run.workspace.repo),
            path,
            task.branch.as_deref(),
        )
        .await
        {
            tracing::warn!(task = %id, "retry: worktree remove failed (continuing): {e}");
        }
    }

    let task = state.store.reset(&id, session.actor()).await?;
    Ok(Json(task))
}
