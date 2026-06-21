//! `POST /workflows/:id/restart` — reset a workflow to run from the beginning.
//!
//! A restart is a deliberate operator override: it forces the workflow's tasks
//! back to `pending` (clearing each one's claim/worktree/commit/heartbeat), so
//! the run derives back to `draft` and can be `start`ed again. It does **not**
//! re-promote roots — the operator presses Start when ready (two explicit steps).
//!
//! Two flags shape how aggressive it is (both default `false` — the safe form):
//! - `include_done`: also reset `done` tasks. Off → done tasks are kept and only
//!   the unfinished part (running/gating/blocked/ready/pending) is reset, so the
//!   workflow resumes rather than redoing committed work.
//! - `remove_worktrees`: also `git worktree remove --force` each reset task's
//!   tree. Off → trees are left for the scheduler to reuse/recreate.
//!
//! Live agents (`running`/`gating`) are always killed first (best-effort) so a
//! reset task doesn't leave an orphaned agent. Requires `Block` (it discards
//! in-flight work, same authority as cancel).

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Status, StoreError};
use serde::Deserialize;

use crate::dto::WorkflowSummary;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Restart options. Both default to `false` (the safe, resume-style restart).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RestartBody {
    /// Also reset tasks that are already `done` (a true from-scratch restart).
    pub include_done: bool,
    /// Also tear down each reset task's git worktree.
    pub remove_worktrees: bool,
}

/// Restart workflow `:id`: kill live agents, optionally remove worktrees, reset
/// its tasks to `pending`. `404` if the workflow is unknown.
pub async fn restart_workflow(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    body: Option<Json<RestartBody>>,
) -> ApiResult<Json<WorkflowSummary>> {
    session.require(Capability::Block, "block", &id)?;
    let Json(opts) = body.unwrap_or_default();

    let run = state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;

    let tasks = state.store.list_run_tasks(&id).await?;
    for task in &tasks {
        // Keep done tasks unless asked to redo them from scratch.
        if task.status == Status::Done && !opts.include_done {
            continue;
        }
        // Already at the start with nothing to clear — skip the churn/event.
        if task.status == Status::Pending {
            continue;
        }

        // Kill the live agent first so a claimed task doesn't leave an orphan.
        if matches!(task.status, Status::Running | Status::Gating)
            && let Err(e) = lazybones_engine::cancel_agent(&task.id).await
        {
            tracing::warn!(task = %task.id, "restart: hcom kill failed (continuing): {e}");
        }

        // Optionally tear down the worktree (and its branch) before we forget
        // their values on reset. The branch must go too, or the next New-mode
        // claim's `worktree add -b <branch>` collides with the leftover branch.
        if opts.remove_worktrees
            && let Some(path) = &task.worktree
            && let Err(e) = lazybones_engine::remove_worktree(
                std::path::Path::new(&run.workspace.repo),
                path,
                task.branch.as_deref(),
            )
            .await
        {
            tracing::warn!(task = %task.id, "restart: worktree remove failed (continuing): {e}");
        }

        if let Err(e) = state.store.reset(&task.id, session.actor()).await {
            tracing::warn!(task = %task.id, "restart: reset failed: {e}");
        }
    }

    // Re-read so the summary reflects the post-restart task statuses.
    let tasks = state.store.list_run_tasks(&id).await?;
    Ok(Json(WorkflowSummary::new(run, &tasks)))
}
