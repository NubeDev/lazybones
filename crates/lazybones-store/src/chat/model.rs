//! The chat conversation model: one append-only thread per task.
//!
//! A task agent and the operator talk on the task's hcom thread; this is the
//! durable, queryable record of that conversation (operator messages written by
//! the chat route, agent replies mirrored in from the hcom tail). It is distinct
//! from the [`Event`](crate::Event) run log (lifecycle transitions) and the
//! [`HcomLogEntry`](crate::HcomLogEntry) raw fabric trace: this is the curated,
//! two-sided conversation a "chat with the agent" view renders.

use serde::{Deserialize, Serialize};

/// Who authored a chat message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// The operator (a human steering the task through the UI).
    User,
    /// The task's agent, replying on its hcom thread.
    Agent,
}

impl ChatRole {
    /// The lowercase wire/storage form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ChatRole::User => "user",
            ChatRole::Agent => "agent",
        }
    }

    /// Parse a stored role string; anything but `agent` is treated as `user`
    /// (the safe default — an unknown author is not attributed to the agent).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "agent" => ChatRole::Agent,
            _ => ChatRole::User,
        }
    }
}

/// One message in a task's conversation (the wire/JSON projection — no SurrealDB
/// types leak out, exactly like [`Event`](crate::Event)).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// The run (workflow id) this conversation belongs to — for grouping only;
    /// the conversation is keyed on `task`.
    pub run: String,
    /// The task whose thread this message is on.
    pub task: String,
    /// Who wrote it.
    pub role: ChatRole,
    /// The message text.
    pub text: String,
    /// RFC3339 timestamp.
    pub at: String,
}
