//! `POST /tasks/:id/ready` — `pending → ready` for one task.
//!
//! The single-task counterpart to `POST /tasks/promote` (which promotes every
//! eligible pending task at once). The board uses this to promote exactly the
//! card the user dragged into the Ready column. Readiness (deps all `done`) is
//! enforced by the caller; the lifecycle only checks that the move `pending →
//! ready` is legal. Requires `Claim`, the same grant the bulk promote holds.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Task, Transition};

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Promote one task `pending → ready`. Requires `Claim`.
pub async fn ready_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Claim, "ready", &id)?;
    let task = state
        .store
        .transition(&id, Transition::Ready, session.actor())
        .await?;
    Ok(Json(task))
}
