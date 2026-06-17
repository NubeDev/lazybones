//! `POST /workflows/:id/cancel` — cancel a workflow.
//!
//! Sets the run's lifecycle to `cancelled`, then stops its tasks: claimed tasks
//! (`running`/`gating`) have their live hcom agent killed (best-effort) and are
//! blocked; unclaimed non-terminal tasks (`pending`/`ready`) are blocked
//! directly. Terminal tasks (`done`/`blocked`) are left alone. Requires `Block`,
//! reusing the same `cancel_agent` primitive as the per-task cancel route.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Status, StoreError, Transition};

use crate::dto::WorkflowSummary;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Cancel workflow `:id` and stop its tasks. `404` if the workflow is unknown.
pub async fn cancel_workflow(
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

    let run = state.store.cancel_run(&id).await?;

    let tasks = state.store.list_run_tasks(&id).await?;
    for task in &tasks {
        if task.status.is_terminal() {
            continue;
        }
        // Kill the live agent first (best-effort) so a claimed task leaves
        // `running` cleanly; an already-dead agent must not block the block.
        if matches!(task.status, Status::Running | Status::Gating)
            && let Err(e) = lazybones_engine::cancel_agent(&task.id).await
        {
            tracing::warn!(task = %task.id, "cancel: hcom kill failed (continuing): {e}");
        }
        let reason = "workflow cancelled by operator".to_owned();
        if let Err(e) = state
            .store
            .transition(&task.id, Transition::Block { reason }, session.actor())
            .await
        {
            tracing::warn!(task = %task.id, "cancel: block failed: {e}");
        }
    }

    // Re-read so the summary reflects the post-cancel task statuses.
    let tasks = state.store.list_run_tasks(&id).await?;
    Ok(Json(WorkflowSummary::new(run, &tasks)))
}
