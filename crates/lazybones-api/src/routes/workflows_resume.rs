//! `POST /workflows/:id/resume` — continue a workflow from where it broke.
//!
//! Resume is the surgical counterpart to [`restart`](super::workflows_restart):
//! where restart resets *all* unfinished tasks (running/gating/blocked/ready/
//! pending) and feels like a redo, resume touches **only the `blocked` tasks**,
//! resetting them to `pending` and leaving everything else — done, running, ready,
//! pending — exactly as it is. So a partly-progressed run that hit a wall keeps its
//! committed work and in-flight agents, and just the stuck tasks re-enter the
//! pipeline (the scheduler auto-promotes them on the next tick).
//!
//! Each blocked task is reset via the shared store
//! [`reset`](lazybones_store::StoreHandle::reset) (the same path retry/restart
//! use), which clears its claim/worktree/commit/heartbeat/reason. A blocked task
//! has no live agent and its worktree is kept for the re-spawn, so unlike restart
//! there is nothing to kill or tear down here. Requires `Block`. `404` if the
//! workflow is unknown.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Status, StoreError};

use crate::dto::WorkflowSummary;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Resume workflow `:id`: reset only its `blocked` tasks → `pending`, leaving
/// running/ready/pending/done untouched. `404` if the workflow is unknown.
pub async fn resume_workflow(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowSummary>> {
    session.require(Capability::Block, "block", &id)?;

    let run = state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;

    let tasks = state.store.list_run_tasks(&id).await?;
    for task in &tasks {
        // Only the stuck tasks re-enter the pipeline; everything else is left as
        // it is so committed/in-flight work is preserved.
        if task.status != Status::Blocked {
            continue;
        }
        if let Err(e) = state.store.reset(&task.id, session.actor()).await {
            tracing::warn!(task = %task.id, "resume: reset failed: {e}");
        }
    }

    // Re-read so the summary reflects the post-resume task statuses.
    let tasks = state.store.list_run_tasks(&id).await?;
    Ok(Json(WorkflowSummary::new(run, &tasks)))
}
