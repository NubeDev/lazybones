//! `GET /workflows/:id` — workflow detail: workspace, derived state, task ids.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_store::StoreError;

use crate::dto::{WorkflowDetail, WorkflowSummary};
use crate::error::ApiResult;
use crate::state::AppState;

/// Fetch a workflow with its derived state, counts, and generated task ids, or
/// `404` if it does not exist.
pub async fn get_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowDetail>> {
    let run = state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;
    let tasks = state.store.list_run_tasks(&id).await?;
    let task_ids = tasks.iter().map(|t| t.id.clone()).collect();
    let summary = WorkflowSummary::new(run, &tasks);
    Ok(Json(WorkflowDetail { summary, task_ids }))
}
