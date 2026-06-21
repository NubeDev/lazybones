//! `GET /skills/:id` — fetch one skill (open read).

use axum::Json;
use axum::extract::{Path, State};
use lazybones_store::{Skill, StoreError};

use crate::error::ApiResult;
use crate::state::AppState;

/// Fetch a skill by id, or `404` if it does not exist.
pub async fn get_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Skill>> {
    let skill = state
        .store
        .get_skill(&id)
        .await?
        .ok_or(StoreError::SkillNotFound(id))?;
    Ok(Json(skill))
}
