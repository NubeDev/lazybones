//! Persisted rows for management-agent conversations and messages.
//!
//! Two tables mirroring the `chat` shape: `agent_conversation` (one per thread)
//! and `agent_message` (append-only, keyed on `conversation_id`). Each row keeps
//! a wire projection that leaks no SurrealDB types. `hcom_id` dedups mirrored
//! agent replies on `(conversation_id, hcom_id)`, exactly like task chat.

use surrealdb::types::{Datetime, RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::{AgentConversation, AgentMessage, AgentRole};

/// The table management-agent conversations live in.
pub(crate) const CONVERSATION_TABLE: &str = "agent_conversation";

/// The table management-agent messages live in.
pub(crate) const MESSAGE_TABLE: &str = "agent_message";

/// One stored conversation.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct ConversationRow {
    pub(crate) id: RecordId,
    /// JSON-serialized page-context snapshot (opaque to the store).
    pub(crate) page_context: Option<String>,
    pub(crate) created_at: Option<String>,
}

impl ConversationRow {
    /// Project to the wire [`AgentConversation`].
    pub(crate) fn into_conversation(self) -> AgentConversation {
        AgentConversation {
            id: record_key(&self.id),
            page_context: self
                .page_context
                .and_then(|s| serde_json::from_str(&s).ok()),
            created_at: self.created_at.unwrap_or_default(),
        }
    }
}

/// One stored message, keyed to its conversation.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct MessageRow {
    pub(crate) id: RecordId,
    pub(crate) conversation_id: String,
    /// `"user" | "agent" | "tool"`.
    pub(crate) role: String,
    pub(crate) text: String,
    pub(crate) at: Datetime,
    /// The source hcom event id for a mirrored agent reply (the dedup key);
    /// `None` for an operator message.
    pub(crate) hcom_id: Option<i64>,
    /// JSON-serialized [`ConfirmAction`] for a `confirm` message; `None` otherwise.
    pub(crate) action: Option<String>,
}

impl MessageRow {
    /// Project to the wire [`AgentMessage`].
    pub(crate) fn into_message(self) -> AgentMessage {
        AgentMessage {
            conversation_id: self.conversation_id,
            role: AgentRole::parse(&self.role),
            text: self.text,
            action: self.action.and_then(|s| serde_json::from_str(&s).ok()),
            at: self.at.to_string(),
        }
    }
}

/// The raw string form of a record id's key (the part after `table:`).
fn record_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
