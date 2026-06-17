//! Wire types for the REST surface (request bodies + the task projection).
//!
//! The domain [`Task`](lazybones_store::Task) already derives serde, so it *is*
//! the task DTO — these are the small request bodies the mutating routes accept.

use serde::Deserialize;

/// `POST /tasks/:id/claim` body: where the agent will work.
#[derive(Debug, Deserialize)]
pub struct ClaimBody {
    /// hcom session id that took the task.
    pub session: String,
    /// The git worktree path the agent edits.
    pub worktree: String,
    /// The branch the agent commits to.
    pub branch: String,
    /// The bearer token to mint for this agent session.
    pub token: String,
}

/// `POST /tasks/:id/done` body: the commit the agent pushed.
#[derive(Debug, Deserialize)]
pub struct DoneBody {
    /// The commit sha that landed on the task branch.
    pub commit: String,
}

/// `POST /tasks/:id/block` body: why it could not finish.
#[derive(Debug, Deserialize)]
pub struct BlockBody {
    /// Human-readable reason, recorded on the task and in the run log.
    pub reason: String,
}
