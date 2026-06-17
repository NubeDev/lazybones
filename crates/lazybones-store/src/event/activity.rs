//! An agent activity message — ephemeral "what the agent is doing right now".
//!
//! Distinct from a lifecycle [`Event`](super::row::Event): a transition is a
//! durable status change recorded in the run log, whereas an activity is a live
//! progress note ("running cargo test…") an agent emits so the user can see the
//! agent is actually working. Activities are pushed on the live bus and surfaced
//! over SSE, but are *not* persisted — they are a signal, not history.

/// A free-form progress message from an agent working a task.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Activity {
    /// The run this activity belongs to.
    pub run: String,
    /// The task the agent is working.
    pub task: String,
    /// Who emitted it (the agent session actor).
    pub actor: String,
    /// The human-readable progress message.
    pub message: String,
    /// RFC3339 timestamp of the message.
    pub at: String,
}
