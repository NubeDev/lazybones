//! `GET /templates/:id` — fetch one template (open read).

use axum::Json;
use axum::extract::{Path, State};
use lazybones_store::{StoreError, Template};

use crate::error::ApiResult;
use crate::state::AppState;

/// Fetch a template by id, or `404` if it does not exist.
pub async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Template>> {
    let template = state
        .store
        .get_template(&id)
        .await?
        .ok_or(StoreError::TemplateNotFound(id))?;
    Ok(Json(template))
}
