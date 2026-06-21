//! `POST /skills` — author a reusable block of agent instructions.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::Skill;

use crate::dto::CreateSkillBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Create a skill. Requires `Author` (loop-only). Returns the created skill, or
/// `409` if the id already exists.
pub async fn create_skill(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateSkillBody>,
) -> ApiResult<Json<Skill>> {
    session.require(Capability::Author, "author", &body.id)?;
    let now = state.store.now();
    let skill = Skill::new(&body.id, &body.title, &body.description, &body.body, now);
    Ok(Json(state.store.create_skill(&skill).await?))
}
