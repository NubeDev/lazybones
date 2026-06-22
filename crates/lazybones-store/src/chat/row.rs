//! A chat-message row — one per message in a task's conversation.
//!
//! Mirrors the [`Event`](crate::Event) row shape: a private SurrealDB row plus a
//! wire projection ([`ChatMessage`]) that leaks no SurrealDB types. `hcom_id` is
//! the dedup key for mirrored agent replies (the source hcom event id); it is
//! `None` for operator messages, which are always fresh inserts.

use surrealdb::types::{Datetime, RecordId, SurrealValue};

use super::model::{ChatMessage, ChatRole};

/// The table chat messages live in.
pub(crate) const CHAT_TABLE: &str = "chat";

/// One stored chat message, keyed to the task whose thread it is on.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct ChatRow {
    pub(crate) id: RecordId,
    /// The run (workflow id) for grouping; the conversation is keyed on `task`.
    pub(crate) run: String,
    /// The task this message belongs to.
    pub(crate) task: String,
    /// `"user" | "agent"`.
    pub(crate) role: String,
    /// The message text.
    pub(crate) text: String,
    /// RFC3339 timestamp.
    pub(crate) at: Datetime,
    /// The source hcom event id for a mirrored agent reply (the dedup key); `None`
    /// for an operator message.
    pub(crate) hcom_id: Option<i64>,
}

impl ChatRow {
    /// Project to the wire [`ChatMessage`].
    pub(crate) fn into_message(self) -> ChatMessage {
        ChatMessage {
            run: self.run,
            task: self.task,
            role: ChatRole::parse(&self.role),
            text: self.text,
            at: self.at.to_string(),
        }
    }
}
