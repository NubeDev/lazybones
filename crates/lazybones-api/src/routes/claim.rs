//! `POST /tasks/:id/claim` — `ready → running`, mint the agent's session token.
//!
//! The loop calls this after `git worktree add`: it records the session, worktree,
//! and branch on the task, and registers a scoped agent token so the agent's later
//! heartbeat/done/block calls authenticate as itself, bound to this one task.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::{Capability, ScopedSession};
use lazybones_gh::Gh;
use lazybones_store::{Task, Transition};

use crate::dto::ClaimBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Claim a ready task and register the agent token. Requires `Claim`.
pub async fn claim_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<ClaimBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Claim, "claim", &id)?;

    // Capture the worktree HEAD *before* the agent runs, so the empty-task gate can
    // later tell "this task advanced HEAD" from "no-op task" even in a shared tree
    // (where the branch always carries prior tasks' commits). Best-effort: a read
    // failure leaves it `None` and the gate falls back to the branch-ahead check.
    let base_commit = Gh::new()
        .git(&body.worktree, ["rev-parse", "HEAD"])
        .await
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let task = state
        .store
        .transition(
            &id,
            Transition::Claim {
                session: body.session.clone(),
                worktree: body.worktree,
                branch: body.branch,
                base_commit,
            },
            session.actor(),
        )
        .await?;

    state.register_agent(body.token, ScopedSession::for_agent(body.session, &id));
    Ok(Json(task))
}
