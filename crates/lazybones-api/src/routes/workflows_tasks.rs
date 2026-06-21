//! `GET /workflows/:id/tasks` — the tasks belonging to one workflow.
//!
//! The server-side counterpart to what the UI used to do client-side (fetch
//! `GET /tasks` and filter by `run_id`). Keying the filter off `run_id` in the
//! store means the UI can only ever receive *this* workflow's tasks — a foreign
//! task cannot leak into a workflow view even if the client filter is wrong or
//! absent. `404` if the workflow is unknown, so a stale id fails loudly instead
//! of silently returning an empty list that looks like "no tasks yet".

use axum::Json;
use axum::extract::{Path, State};
use lazybones_store::{StoreError, Task};

use crate::error::ApiResult;
use crate::state::AppState;

/// List the tasks linked to workflow `:id` via `run_id`. Reads are open (no
/// token), matching `GET /tasks` and `GET /workflows/:id`. `404` if the
/// workflow does not exist.
pub async fn list_workflow_tasks(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Task>>> {
    // The workflow must exist, so an unknown id 404s rather than masquerading as
    // an empty workflow.
    state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;

    Ok(Json(state.store.list_run_tasks(&id).await?))
}
