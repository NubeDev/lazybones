//! `GET /workflows` — list workflows with derived state + task counts (open read).

use axum::Json;
use axum::extract::State;

use crate::dto::WorkflowSummary;
use crate::error::ApiResult;
use crate::state::AppState;

/// List every workflow, each with its derived state and task counts.
pub async fn list_workflows(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<WorkflowSummary>>> {
    let runs = state.store.list_runs().await?;
    let mut out = Vec::with_capacity(runs.len());
    for run in runs {
        let tasks = state.store.list_run_tasks(&run.id).await?;
        out.push(WorkflowSummary::new(run, &tasks));
    }
    Ok(Json(out))
}
