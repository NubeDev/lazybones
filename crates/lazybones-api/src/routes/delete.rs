//! `DELETE /tasks/:id` — remove a task and its dependency edges.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::StoreError;

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Delete a task (and its `depends_on` edges, both directions). Requires
/// `Author` (loop-only). Returns `404` if no such task existed.
pub async fn delete_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Author, "author", &id)?;
    let existed = state.store.delete_task(&id).await?;
    if !existed {
        return Err(StoreError::TaskNotFound(id).into());
    }
    Ok(Json(serde_json::json!({ "deleted": true })))
}
