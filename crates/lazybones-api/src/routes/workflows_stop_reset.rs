//! `POST /workflows/:id/stop-reset` — pause a workflow AND reset its in-flight
//! progress.
//!
//! The heavier sibling of [`stop`](super::workflows_stop): where stop reclaims
//! running tasks to `ready` keeping their work, stop-reset is "throw the in-flight
//! progress away". It flips the run's `lifecycle` to `stopped` (so the scheduler
//! promotes/claims nothing) and resets every **unfinished** task back to
//! `pending` — killing any live agent first (best-effort) and clearing each
//! task's claim/worktree/commit/heartbeat/reason via the shared store
//! [`reset`](lazybones_store::StoreHandle::reset). Terminal `done` tasks are kept.
//!
//! It is still **not** terminal: [`resume`](super::workflows_resume) flips the run
//! back to `active` and the scheduler re-promotes from the reset `pending` state.
//! (Delete is the only archive path.) It differs from
//! [`restart`](super::workflows_restart) only in that restart leaves the run
//! `active` and re-promotes immediately, while stop-reset leaves it paused until a
//! deliberate resume. Requires `Block`. `404` if the workflow is unknown.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Status, StoreError};

use crate::dto::WorkflowSummary;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Stop-and-reset workflow `:id`: lifecycle → `stopped`, kill live agents, reset
/// every unfinished task to `pending` (done tasks kept). `404` if unknown.
pub async fn stop_reset_workflow(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowSummary>> {
    session.require(Capability::Block, "block", &id)?;

    state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;

    // Pause first so the scheduler stops claiming this run while we reset it.
    let run = state.store.stop_run(&id).await?;

    let tasks = state.store.list_run_tasks(&id).await?;
    for task in &tasks {
        // Keep finished work; reset everything still in flight (running/gating/
        // blocked/ready). A `pending` task is already at the start — skip the churn.
        if task.status == Status::Done || task.status == Status::Pending {
            continue;
        }
        // Kill the live agent first so a claimed task doesn't leave an orphan.
        if matches!(task.status, Status::Running | Status::Gating)
            && let Err(e) = lazybones_engine::cancel_agent(&task.id).await
        {
            tracing::warn!(task = %task.id, "stop-reset: hcom kill failed (continuing): {e}");
        }
        if let Err(e) = state.store.reset(&task.id, session.actor()).await {
            tracing::warn!(task = %task.id, "stop-reset: reset failed: {e}");
        }
    }

    // Re-read so the summary reflects the post-reset task statuses.
    let tasks = state.store.list_run_tasks(&id).await?;
    Ok(Json(WorkflowSummary::new(run, &tasks)))
}
