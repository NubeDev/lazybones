//! `POST /tasks/:id/activity` — a free-form agent progress message.
//!
//! Distinct from a heartbeat (liveness) and from a transition (lifecycle): this
//! broadcasts an ephemeral "what I'm doing right now" note on the live feed
//! (`activity` SSE event) so the user can watch the agent work. Nothing is
//! persisted — it is a signal, not history. Scoped by the `Heartbeat` capability
//! (an agent reporting on its own task), bound to the task.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::StoreError;
use serde_json::{Value, json};

use crate::dto::ActivityBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Broadcast a progress message for `id` on the live feed.
pub async fn report_activity(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<ActivityBody>,
) -> ApiResult<Json<Value>> {
    session.require(Capability::Heartbeat, "activity", &id)?;
    let task = state
        .store
        .get_task(&id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.clone()))?;
    state
        .store
        .report_activity(&task.run, &id, session.actor(), &body.message);
    Ok(Json(json!({ "status": "ok" })))
}
