//! Append one message to a task's conversation.
//!
//! Operator messages (`hcom_id = None`) always insert. Mirrored agent replies
//! carry the source hcom event id and are idempotent on `(task, hcom_id)`: a
//! re-drain of the hcom tail (a crash mid-tail) re-mirrors the same reply as a
//! no-op rather than a duplicate, matching the hcom log's own at-least-once
//! ingestion contract.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, SurrealValue};

use crate::error::{Result, StoreError};

use super::model::{ChatMessage, ChatRole};
use super::row::{CHAT_TABLE, ChatRow};

/// The content of a new chat row (no `id` — SurrealDB mints a ULID key).
#[derive(Debug, Clone, SurrealValue)]
struct NewChat {
    run: String,
    task: String,
    role: String,
    text: String,
    at: Datetime,
    hcom_id: Option<i64>,
}

/// Append `text` from `role` to `task`'s conversation in `run`, returning the
/// persisted [`ChatMessage`] (for live publication on the bus).
///
/// When `hcom_id` is `Some` (a mirrored agent reply), the write is deduped on
/// `(task, hcom_id)` — an existing row is returned unchanged.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails.
pub async fn append_chat(
    db: &Surreal<Db>,
    run: &str,
    task: &str,
    role: ChatRole,
    text: &str,
    hcom_id: Option<i64>,
) -> Result<ChatMessage> {
    // Dedup mirrored replies on (task, hcom_id): if the row exists, return it.
    if let Some(hid) = hcom_id {
        let existing: Vec<ChatRow> = db
            .query(format!(
                "SELECT * FROM {CHAT_TABLE} WHERE task = $task AND hcom_id = $hcom_id LIMIT 1"
            ))
            .bind(("task", task.to_owned()))
            .bind(("hcom_id", hid))
            .await
            .map_err(StoreError::Operation)?
            .take(0)
            .map_err(StoreError::Operation)?;
        if let Some(row) = existing.into_iter().next() {
            return Ok(row.into_message());
        }
    }

    let written: Option<ChatRow> = db
        .create(CHAT_TABLE)
        .content(NewChat {
            run: run.to_owned(),
            task: task.to_owned(),
            role: role.as_str().to_owned(),
            text: text.to_owned(),
            at: Datetime::now(),
            hcom_id,
        })
        .await
        .map_err(StoreError::Operation)?;
    written.map(ChatRow::into_message).ok_or_else(|| {
        StoreError::Operation(surrealdb::Error::thrown(
            "chat insert returned no row".to_owned(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;

    async fn db() -> Surreal<Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn user_messages_always_insert() {
        let db = db().await;
        append_chat(&db, "wf", "t", ChatRole::User, "hi", None).await.unwrap();
        append_chat(&db, "wf", "t", ChatRole::User, "hi", None).await.unwrap();
        let rows: Vec<ChatRow> = db
            .query(format!("SELECT * FROM {CHAT_TABLE} WHERE task = 't'"))
            .await
            .unwrap()
            .take(0)
            .unwrap();
        assert_eq!(rows.len(), 2, "identical user messages are distinct rows");
    }

    #[tokio::test]
    async fn agent_replies_dedup_on_hcom_id() {
        let db = db().await;
        let a = append_chat(&db, "wf", "t", ChatRole::Agent, "reply", Some(7))
            .await
            .unwrap();
        let b = append_chat(&db, "wf", "t", ChatRole::Agent, "reply", Some(7))
            .await
            .unwrap();
        assert_eq!(a, b);
        let rows: Vec<ChatRow> = db
            .query(format!("SELECT * FROM {CHAT_TABLE} WHERE task = 't'"))
            .await
            .unwrap()
            .take(0)
            .unwrap();
        assert_eq!(rows.len(), 1, "re-mirrored agent reply is a no-op");
    }
}
