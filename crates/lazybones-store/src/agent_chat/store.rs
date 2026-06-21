//! The verbs over management-agent conversations and messages.
//!
//! Conversations get a SurrealDB-minted id; messages are append-only and keyed
//! on `conversation_id`. Mirrored agent replies are deduped on
//! `(conversation_id, hcom_id)` — a re-drain of the hcom tail re-mirrors the same
//! reply as a no-op, matching the task-chat ingestion contract.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, SurrealValue};

use crate::error::{Result, StoreError};

use super::model::{AgentConversation, AgentMessage, AgentRole, ConfirmAction};
use super::row::{
    CONVERSATION_TABLE, ConversationRow, MESSAGE_TABLE, MessageRow,
};

/// The content of a new conversation (no `id` — SurrealDB mints a ULID key).
#[derive(Debug, Clone, SurrealValue)]
struct NewConversation {
    page_context: Option<String>,
    created_at: String,
}

/// The content of a new message (no `id` — SurrealDB mints a ULID key).
#[derive(Debug, Clone, SurrealValue)]
struct NewMessage {
    conversation_id: String,
    role: String,
    text: String,
    at: Datetime,
    hcom_id: Option<i64>,
    action: Option<String>,
}

/// Open a new conversation, optionally snapshotting the page context. Returns
/// the persisted [`AgentConversation`] (its minted id is the stream key).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn create_agent_conversation(
    db: &Surreal<Db>,
    page_context: Option<&serde_json::Value>,
    now: &str,
) -> Result<AgentConversation> {
    let written: Option<ConversationRow> = db
        .create(CONVERSATION_TABLE)
        .content(NewConversation {
            page_context: page_context.map(ToString::to_string),
            created_at: now.to_owned(),
        })
        .await
        .map_err(StoreError::Operation)?;
    written
        .map(ConversationRow::into_conversation)
        .ok_or_else(|| {
            StoreError::Operation(surrealdb::Error::thrown(
                "conversation insert returned no row".to_owned(),
            ))
        })
}

/// Read a single conversation by id, or `None` if it does not exist.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_agent_conversation(
    db: &Surreal<Db>,
    id: &str,
) -> Result<Option<AgentConversation>> {
    let row: Option<ConversationRow> = db
        .select((CONVERSATION_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(ConversationRow::into_conversation))
}

/// List all conversations, newest first.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn list_agent_conversations(db: &Surreal<Db>) -> Result<Vec<AgentConversation>> {
    let rows: Vec<ConversationRow> = db
        .query(format!(
            "SELECT * FROM {CONVERSATION_TABLE} ORDER BY created_at DESC"
        ))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(ConversationRow::into_conversation).collect())
}

/// Append `text` from `role` to `conversation_id`, returning the persisted
/// [`AgentMessage`] (for live publication on the bus).
///
/// When `hcom_id` is `Some` (a mirrored agent reply), the write is deduped on
/// `(conversation_id, hcom_id)` — an existing row is returned unchanged.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails.
pub async fn append_agent_message(
    db: &Surreal<Db>,
    conversation_id: &str,
    role: AgentRole,
    text: &str,
    hcom_id: Option<i64>,
) -> Result<AgentMessage> {
    if let Some(hid) = hcom_id {
        let existing: Vec<MessageRow> = db
            .query(format!(
                "SELECT * FROM {MESSAGE_TABLE} WHERE conversation_id = $cid AND hcom_id = $hcom_id LIMIT 1"
            ))
            .bind(("cid", conversation_id.to_owned()))
            .bind(("hcom_id", hid))
            .await
            .map_err(StoreError::Operation)?
            .take(0)
            .map_err(StoreError::Operation)?;
        if let Some(row) = existing.into_iter().next() {
            return Ok(row.into_message());
        }
    }

    let written: Option<MessageRow> = db
        .create(MESSAGE_TABLE)
        .content(NewMessage {
            conversation_id: conversation_id.to_owned(),
            role: role.as_str().to_owned(),
            text: text.to_owned(),
            at: Datetime::now(),
            hcom_id,
            action: None,
        })
        .await
        .map_err(StoreError::Operation)?;
    written.map(MessageRow::into_message).ok_or_else(|| {
        StoreError::Operation(surrealdb::Error::thrown(
            "agent message insert returned no row".to_owned(),
        ))
    })
}

/// Append a gated `confirm` message proposing `action` (summary in `text`),
/// returning the persisted [`AgentMessage`]. Deduped on `(conversation_id,
/// hcom_id)` like a mirrored reply, so a re-drained CONFIRM line is a no-op.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails.
pub async fn append_confirm_request(
    db: &Surreal<Db>,
    conversation_id: &str,
    text: &str,
    action: &ConfirmAction,
    hcom_id: Option<i64>,
) -> Result<AgentMessage> {
    if let Some(hid) = hcom_id {
        let existing: Vec<MessageRow> = db
            .query(format!(
                "SELECT * FROM {MESSAGE_TABLE} WHERE conversation_id = $cid AND hcom_id = $hcom_id LIMIT 1"
            ))
            .bind(("cid", conversation_id.to_owned()))
            .bind(("hcom_id", hid))
            .await
            .map_err(StoreError::Operation)?
            .take(0)
            .map_err(StoreError::Operation)?;
        if let Some(row) = existing.into_iter().next() {
            return Ok(row.into_message());
        }
    }

    let action_json = serde_json::to_string(action)
        .map_err(|e| StoreError::Operation(surrealdb::Error::thrown(e.to_string())))?;
    let written: Option<MessageRow> = db
        .create(MESSAGE_TABLE)
        .content(NewMessage {
            conversation_id: conversation_id.to_owned(),
            role: AgentRole::Confirm.as_str().to_owned(),
            text: text.to_owned(),
            at: Datetime::now(),
            hcom_id,
            action: Some(action_json),
        })
        .await
        .map_err(StoreError::Operation)?;
    written.map(MessageRow::into_message).ok_or_else(|| {
        StoreError::Operation(surrealdb::Error::thrown(
            "confirm request insert returned no row".to_owned(),
        ))
    })
}

/// Read a conversation's full message history, oldest first.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn agent_message_history(
    db: &Surreal<Db>,
    conversation_id: &str,
) -> Result<Vec<AgentMessage>> {
    let rows: Vec<MessageRow> = db
        .query(format!(
            "SELECT * FROM {MESSAGE_TABLE} WHERE conversation_id = $cid ORDER BY at ASC"
        ))
        .bind(("cid", conversation_id.to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(MessageRow::into_message).collect())
}
