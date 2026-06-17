//! `GET /templates` — list reusable task templates (open read).

use axum::Json;
use axum::extract::State;
use lazybones_store::Template;

use crate::error::ApiResult;
use crate::state::AppState;

/// List every task template.
pub async fn list_templates(State(state): State<AppState>) -> ApiResult<Json<Vec<Template>>> {
    Ok(Json(state.store.list_templates().await?))
}
