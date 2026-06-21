//! `GET /skills` — list reusable agent-instruction skills (open read).

use axum::Json;
use axum::extract::State;
use lazybones_store::Skill;

use crate::error::ApiResult;
use crate::state::AppState;

/// List every skill.
pub async fn list_skills(State(state): State<AppState>) -> ApiResult<Json<Vec<Skill>>> {
    Ok(Json(state.store.list_skills().await?))
}
