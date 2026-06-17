//! `GET /runs/:id` — the full transition history for a run.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_store::Event;

use crate::error::ApiResult;
use crate::state::AppState;

/// Every recorded transition for `run`, oldest first.
pub async fn run_history(
    State(state): State<AppState>,
    Path(run): Path<String>,
) -> ApiResult<Json<Vec<Event>>> {
    let events = state.store.run_history(&run).await?;
    Ok(Json(events))
}
