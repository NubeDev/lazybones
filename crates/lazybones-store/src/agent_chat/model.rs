//! The Lazybones-Agent conversation model — durable chat history.
//!
//! Distinct from the per-task [`ChatMessage`](crate::ChatMessage): this is the
//! global management agent's conversation surface. A conversation groups an
//! append-only thread of messages; history survives reload and is auditable
//! (`docs/agent/lazybones-agent-scope.md` §8.3).

use serde::{Deserialize, Serialize};

/// Who authored a management-agent message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    /// The operator (a human steering the agent through the chat panel).
    User,
    /// The management agent's reply.
    Agent,
    /// A transparency note about a REST action the agent took (scope §8.4).
    Tool,
    /// A gated lifecycle action the agent is *proposing*; the human confirms it
    /// in the UI (the message's `action` carries the exact REST call, §10.2).
    Confirm,
}

impl AgentRole {
    /// The lowercase wire/storage form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AgentRole::User => "user",
            AgentRole::Agent => "agent",
            AgentRole::Tool => "tool",
            AgentRole::Confirm => "confirm",
        }
    }

    /// Parse a stored role string; anything unknown is treated as `user` (the
    /// safe default — never mis-attribute to the agent).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "agent" => AgentRole::Agent,
            "tool" => AgentRole::Tool,
            "confirm" => AgentRole::Confirm,
            _ => AgentRole::User,
        }
    }
}

/// A gated lifecycle action the agent proposes — the exact REST call the UI will
/// issue (under the operator's token) if the human confirms it (scope §10.2).
/// Kept deliberately literal: the agent describes the call, the UI makes it; the
/// agent's own token never carries lifecycle power.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfirmAction {
    /// A short verb for the UI label, e.g. `"start"`, `"retry"`, `"delete"`.
    pub action: String,
    /// The HTTP method, e.g. `"POST"`, `"PUT"`, `"DELETE"`.
    pub method: String,
    /// The REST path, e.g. `"/workflows/add-healthcheck/start"`.
    pub path: String,
    /// An optional JSON request body for the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

/// One conversation between the operator and the management agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConversation {
    /// Stable conversation id (SurrealDB-minted ULID key).
    pub id: String,
    /// A JSON snapshot of the page context this conversation opened on (scope
    /// §7) — opaque to the store, rendered by the runner; `None` if global.
    pub page_context: Option<serde_json::Value>,
    /// RFC3339 timestamp the conversation was created.
    pub created_at: String,
}

/// One message in a management-agent conversation (the wire/JSON projection — no
/// SurrealDB types leak out).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    /// The conversation this message belongs to.
    pub conversation_id: String,
    /// Who wrote it.
    pub role: AgentRole,
    /// The message text. For a `confirm` message this is the human-readable
    /// summary of the proposed action.
    pub text: String,
    /// The gated lifecycle action this message proposes, present only for the
    /// `confirm` role (scope §10.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ConfirmAction>,
    /// RFC3339 timestamp.
    pub at: String,
}
