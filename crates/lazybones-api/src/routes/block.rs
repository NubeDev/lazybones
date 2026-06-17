//! `POST /tasks/:id/block` — `* → blocked` with a reason.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Task, Transition};

use crate::dto::BlockBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Block a task, recording the reason. Requires `Block`.
pub async fn block_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<BlockBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Block, "block", &id)?;
    let task = state
        .store
        .transition(
            &id,
            Transition::Block {
                reason: body.reason,
            },
            session.actor(),
        )
        .await?;
    Ok(Json(task))
}
