//! `POST /tasks/:id/heartbeat` — agent liveness ping.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::StoreError;
use serde_json::{Value, json};

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Stamp the task's heartbeat. Requires `Heartbeat`, bound to the task.
pub async fn heartbeat(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    session.require(Capability::Heartbeat, "heartbeat", &id)?;
    if !state.store.heartbeat(&id).await? {
        return Err(StoreError::TaskNotFound(id).into());
    }
    Ok(Json(json!({ "status": "ok" })))
}
