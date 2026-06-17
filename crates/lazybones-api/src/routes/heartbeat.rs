//! `POST /tasks/:id/heartbeat` — agent liveness ping, with an optional progress
//! note that is broadcast on the live feed so the user sees the agent working.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::StoreError;
use serde_json::{Value, json};

use crate::dto::HeartbeatBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Stamp the task's heartbeat. Requires `Heartbeat`, bound to the task.
///
/// The body is optional (an empty ping is valid). If it carries a `note`, that
/// note is published as an `activity` event on the live SSE feed.
pub async fn heartbeat(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    body: Option<Json<HeartbeatBody>>,
) -> ApiResult<Json<Value>> {
    session.require(Capability::Heartbeat, "heartbeat", &id)?;
    let task = state
        .store
        .get_task(&id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.clone()))?;
    state.store.heartbeat(&id).await?;

    if let Some(Json(HeartbeatBody { note: Some(note) })) = body
        && !note.trim().is_empty()
    {
        state
            .store
            .report_activity(&task.run, &id, session.actor(), &note);
    }
    Ok(Json(json!({ "status": "ok" })))
}
