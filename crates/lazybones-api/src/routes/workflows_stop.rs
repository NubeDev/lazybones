//! `POST /workflows/:id/stop` — pause a workflow without losing work.
//!
//! Stop is the light, fully-reversible pause (the counterpart to
//! [`stop_reset`](super::workflows_stop_reset), which throws in-flight progress
//! away). It flips the run's `lifecycle` to `stopped` — so the scheduler promotes
//! and claims nothing for it — then quiesces its live tasks: each `running`/
//! `gating` task has its agent killed (best-effort) and is **reclaimed** back to
//! `ready` (status kept, worktree kept — no work is discarded), exactly the path
//! the scheduler uses for a stale agent. Unclaimed tasks (`pending`/`ready`) and
//! terminal/blocked tasks are left untouched.
//!
//! Nothing here is terminal: [`resume`](super::workflows_resume) flips the run
//! back to `active` and the scheduler picks up from the reclaimed `ready` tasks.
//! Requires `Block` (the operator task-control capability). `404` if unknown.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Status, StoreError, Transition};

use crate::dto::WorkflowSummary;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Stop (pause) workflow `:id`: lifecycle → `stopped`, kill live agents, reclaim
/// `running`/`gating` tasks to `ready` keeping their work. `404` if unknown.
pub async fn stop_workflow(
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

    // Pause first so the scheduler stops promoting/claiming this run immediately,
    // even before we finish quiescing the in-flight tasks below.
    let run = state.store.stop_run(&id).await?;

    let tasks = state.store.list_run_tasks(&id).await?;
    for task in &tasks {
        // Only in-flight tasks need quiescing; pending/ready/blocked/done are left
        // exactly as they are (the lifecycle pause already stops them advancing).
        if !matches!(task.status, Status::Running | Status::Gating) {
            continue;
        }
        // Kill the live agent first (best-effort) so reclaim doesn't leave an
        // orphan; an already-dead agent must not block the reclaim.
        if let Err(e) = lazybones_engine::cancel_agent(&task.id).await {
            tracing::warn!(task = %task.id, "stop: hcom kill failed (continuing): {e}");
        }
        // Reclaim (running/gating → ready), keeping the worktree and committed
        // work — no progress is thrown away, unlike stop-reset/restart.
        if let Err(e) = state
            .store
            .transition(&task.id, Transition::Reclaim, session.actor())
            .await
        {
            tracing::warn!(task = %task.id, "stop: reclaim failed: {e}");
        }
    }

    // Re-read so the summary reflects the post-stop task statuses.
    let tasks = state.store.list_run_tasks(&id).await?;
    Ok(Json(WorkflowSummary::new(run, &tasks)))
}
