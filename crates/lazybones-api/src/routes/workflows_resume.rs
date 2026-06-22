//! `POST /workflows/:id/resume` — un-pause a workflow and continue from where it
//! broke.
//!
//! Resume is the un-stop: it is how a `stopped` (paused) run comes back to life,
//! and it is also the surgical counterpart to [`restart`](super::workflows_restart)
//! for a run that merely hit a wall. It does two things:
//!
//! 1. Flips the run's `lifecycle` back to `active` — so the scheduler resumes
//!    promoting/claiming wherever the tasks were left (a [`stop`](super::workflows_stop)
//!    leaves them `ready`; a [`stop_reset`](super::workflows_stop_reset) leaves
//!    them `pending`). This is the lever that releases the scheduler guard, so a
//!    task in a stopped run can only be revived *after* the run is resumed.
//! 2. Resets **only the `blocked` tasks** to `pending`, leaving everything else —
//!    done, running, ready, pending — exactly as it is. So a partly-progressed run
//!    keeps its committed work and in-flight agents, and just the stuck tasks
//!    re-enter the pipeline.
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

/// Resume workflow `:id`: flip lifecycle → `active` and reset only its `blocked`
/// tasks → `pending`, leaving running/ready/pending/done untouched. `404` if the
/// workflow is unknown.
pub async fn resume_workflow(
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

    // Un-pause first: flip lifecycle → active so the scheduler starts promoting/
    // claiming this run again (and the task-level revive verbs stop refusing).
    let run = state.store.resume_run(&id).await?;

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
