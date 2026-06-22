//! `PUT /templates/:id` — edit a reusable task template.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::Template;

use crate::dto::UpdateTemplateBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Edit a template. Requires `Author` (loop-only). Returns the updated template,
/// or `404` if no template with that id exists. The id is taken from the path;
/// `created_at` is preserved by the store.
pub async fn update_template(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<UpdateTemplateBody>,
) -> ApiResult<Json<Template>> {
    session.require(Capability::Author, "author", &id)?;
    let now = state.store.now();
    // `created_at` here is a placeholder; the store preserves the original.
    let template = Template::new(
        &id,
        &body.title,
        &body.description,
        &body.spec_template,
        body.default_tool.clone(),
        body.default_model.clone(),
        body.default_effort.clone(),
        body.default_worktree_mode,
        now,
    );
    Ok(Json(state.store.update_template(&template).await?))
}
