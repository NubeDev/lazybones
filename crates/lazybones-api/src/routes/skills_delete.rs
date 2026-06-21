//! `DELETE /skills/:id` — remove a skill.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Delete a skill. Requires `Author` (loop-only). Returns whether it existed.
pub async fn delete_skill(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Author, "author", &id)?;
    let existed = state.store.delete_skill(&id).await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}
