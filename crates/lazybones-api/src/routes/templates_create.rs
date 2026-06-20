//! `POST /templates` — author a reusable task template.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::Template;

use crate::dto::CreateTemplateBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Create a template. Requires `Author` (loop-only). Returns the created
/// template, or `409` if the id already exists.
pub async fn create_template(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateTemplateBody>,
) -> ApiResult<Json<Template>> {
    session.require(Capability::Author, "author", &body.id)?;
    let now = state.store.now();
    let template = Template::new(
        &body.id,
        &body.title,
        &body.description,
        &body.spec_template,
        body.default_tool.clone(),
        body.default_model.clone(),
        body.default_effort.clone(),
        body.default_worktree_mode,
        now,
    );
    Ok(Json(state.store.create_template(&template).await?))
}
