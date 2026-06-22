//! `PUT /skills/:id` — edit a reusable agent-instruction skill.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::Skill;

use crate::dto::UpdateSkillBody;
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// Edit a skill. Requires `Author` (loop-only). Returns the updated skill, or
/// `404` if no skill with that id exists, or `400` if a structured action is
/// malformed. The id is taken from the path; `created_at` is preserved.
pub async fn update_skill(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<UpdateSkillBody>,
) -> ApiResult<Json<Skill>> {
    session.require(Capability::Author, "author", &id)?;
    let now = state.store.now();
    // `created_at` here is a placeholder; the store preserves the original.
    let mut skill = Skill::new(&id, &body.title, &body.description, &body.body, now);
    if let Some(action) = body.action {
        action.validate().map_err(ApiError::bad_request)?;
        skill = skill.with_action(action);
    }
    Ok(Json(state.store.update_skill(&skill).await?))
}
