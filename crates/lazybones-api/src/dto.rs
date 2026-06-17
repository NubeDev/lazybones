//! Wire types for the REST surface (request bodies + the task projection).
//!
//! The domain [`Task`](lazybones_store::Task) already derives serde, so it *is*
//! the task DTO — these are the small request bodies the mutating routes accept.

use serde::Deserialize;

use lazybones_store::WorktreeMode;

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

/// `POST /tasks/:id/heartbeat` body: optional liveness payload.
///
/// Backward-compatible: an empty body still pings. A `note` rides along as an
/// agent progress message, broadcast on the live feed (`activity` SSE event) so
/// the user can see the agent working.
#[derive(Debug, Default, Deserialize)]
pub struct HeartbeatBody {
    /// Optional progress message ("running cargo test…").
    #[serde(default)]
    pub note: Option<String>,
}

/// `POST /tasks/:id/activity` body: a free-form agent progress message.
#[derive(Debug, Deserialize)]
pub struct ActivityBody {
    /// The human-readable progress message to broadcast on the live feed.
    pub message: String,
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

/// `POST /tasks/:id/cancel` body: an optional reason (defaults when omitted).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CancelBody {
    /// Why the task was cancelled; a blank/absent value records a default.
    pub reason: Option<String>,
}

/// `POST /tasks` body: a new task to author (status starts `Pending`).
#[derive(Debug, Deserialize)]
pub struct CreateTaskBody {
    /// The unique task id; `409` if it is already taken.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// The spec text (the agent's brief).
    pub spec: String,
    /// Ids of tasks this one depends on; wired as graph edges.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Paths/areas this task owns, for conflict avoidance.
    #[serde(default)]
    pub owns: Vec<String>,
    /// The agent tool that should run this task, if pinned.
    #[serde(default)]
    pub tool: Option<String>,
    /// How the loop should provision the worktree on claim; defaults to `new`.
    #[serde(default)]
    pub worktree_mode: WorktreeMode,
}

/// `PATCH /tasks/:id` body: overwrite the authored fields of a task.
#[derive(Debug, Deserialize)]
pub struct UpdateTaskBody {
    /// New title.
    pub title: String,
    /// New spec text.
    pub spec: String,
    /// New dependency ids; edges are reconciled against the old set.
    #[serde(default)]
    pub deps: Vec<String>,
    /// New owned paths/areas.
    #[serde(default)]
    pub owns: Vec<String>,
    /// New pinned agent tool, if any.
    #[serde(default)]
    pub tool: Option<String>,
    /// New worktree provisioning intent; defaults to `new`.
    #[serde(default)]
    pub worktree_mode: WorktreeMode,
}

/// `PUT /secrets/:tool` body: the credential to seal for an agent CLI.
#[derive(Debug, Deserialize)]
pub struct SecretBody {
    /// The environment variable the agent CLI reads (e.g. `ANTHROPIC_API_KEY`).
    pub env_var: String,
    /// The secret value (API key / token). Sealed at rest; never read back.
    pub value: String,
}
