//! The task lifecycle state machine (SCOPE.md "Task lifecycle").
//!
//! ```text
//!   pending --deps met--> ready --claim--> running --DONE--> gating --green--> done
//!     any state --unrecoverable--> blocked
//!     running --stale + no agent--> ready (reclaim)
//! ```

use serde::{Deserialize, Serialize};

/// A task's position in the lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Imported but dependencies are not yet all `done`.
    Pending,
    /// Dependencies met; eligible to be claimed by the loop.
    Ready,
    /// Claimed by an agent in a worktree; work in flight.
    Running,
    /// Agent signalled DONE; the orchestrator is re-running the gate.
    Gating,
    /// Committed, pushed, and the gate re-ran green.
    Done,
    /// Unrecoverable; a reason is recorded and the worktree kept for triage.
    Blocked,
}

impl Status {
    /// The lowercase wire/string form stored in the DB and returned over REST.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Ready => "ready",
            Status::Running => "running",
            Status::Gating => "gating",
            Status::Done => "done",
            Status::Blocked => "blocked",
        }
    }

    /// Whether `self -> to` is a legal transition.
    ///
    /// `blocked` is reachable from any non-terminal state. `done` is terminal.
    /// `running -> ready` is the reclaim path for a stale agent, and `blocked ->
    /// ready` is the operator revive path (workshop a failed task back to life).
    #[must_use]
    pub fn can_transition(self, to: Status) -> bool {
        use Status::{Blocked, Done, Gating, Pending, Ready, Running};
        match (self, to) {
            (Pending, Ready)
            | (Ready, Running)
            | (Running, Gating)
            | (Running, Ready)
            | (Gating, Done)
            | (Gating, Ready)
            // Revive: an operator workshops a blocked task back to life. The only
            // edge out of the otherwise-terminal `blocked` state, and a deliberate
            // operator override (mirrors how `running -> ready` reclaims a stale
            // agent) — the worktree is kept, so the next claim resumes in place.
            | (Blocked, Ready) => true,
            // Any non-terminal state can be blocked.
            (Pending | Ready | Running | Gating, Blocked) => true,
            _ => false,
        }
    }

    /// Terminal states never transition again.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Status::Done | Status::Blocked)
    }
}
