//! `POST /tasks/:id/done` — `gating → done`, record the pushed commit.
//!
//! Only a task already in `gating` (the orchestrator put it there after the agent
//! signalled DONE) can reach `done`; the store's state machine rejects any other
//! source state with `409`. A green gate is what earns this call.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Task, Transition};

use crate::dto::DoneBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Flip a gating task to done with its commit sha. Requires `Done`.
pub async fn done_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<DoneBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Done, "done", &id)?;
    let task = state
        .store
        .transition(
            &id,
            Transition::Done {
                commit: body.commit,
            },
            session.actor(),
        )
        .await?;
    Ok(Json(task))
}
