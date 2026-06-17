//! `POST /tasks/:id/cancel` — stop a running task's agent and block it.
//!
//! The two-step control surface from docs/scheduler.md "Cancellation": kill every
//! hcom agent tagged with the task id, then record a `Block` so the task shows up
//! in the UI with a reason. The kill is best-effort — an already-dead agent must
//! not stop the task from being blocked.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Task, Transition};

use crate::dto::CancelBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Cancel a task: `hcom kill tag:<id>` then `* → blocked`. Requires `Block`.
pub async fn cancel_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<CancelBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Block, "cancel", &id)?;

    // Best-effort: a kill failure (agent already gone, hcom absent) is logged but
    // never blocks the block — the task must still leave `running`.
    if let Err(e) = lazybones_engine::cancel_agent(&id).await {
        tracing::warn!(task = %id, "cancel: hcom kill failed (continuing to block): {e}");
    }

    let reason = body
        .reason
        .filter(|r| !r.trim().is_empty())
        .unwrap_or_else(|| "cancelled by operator".to_owned());
    let task = state
        .store
        .transition(&id, Transition::Block { reason }, session.actor())
        .await?;
    Ok(Json(task))
}
