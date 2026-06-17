//! `GET /tasks/:id` — one task: spec text, status, deps, claim state.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_store::{StoreError, Task};

use crate::error::ApiResult;
use crate::state::AppState;

/// Read one task, or `404` if no such task exists.
pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Task>> {
    let task = state
        .store
        .get_task(&id)
        .await?
        .ok_or(StoreError::TaskNotFound(id))?;
    Ok(Json(task))
}
