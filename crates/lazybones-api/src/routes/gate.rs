//! `POST /tasks/:id/gate` — `running → gating`, loop-driven.
//!
//! The loop calls this after it sees the agent's DONE event, to mark that the
//! orchestrator is now re-running the gate in the worktree. Only the loop holds
//! `Claim` (agents do not), so this route is effectively loop-only.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Task, Transition};

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Move a running task into gating. Requires `Claim` (loop-only).
pub async fn gate_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Claim, "gate", &id)?;
    let task = state
        .store
        .transition(&id, Transition::Gate, session.actor())
        .await?;
    Ok(Json(task))
}
