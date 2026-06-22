//! Read the full conversation for a task (`GET /tasks/:id/chat`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::ChatMessage;
use super::row::{CHAT_TABLE, ChatRow};

/// Every message in `task`'s conversation, oldest first.
///
/// Keyed purely on `task` (not the run) so the conversation is unambiguous
/// regardless of how a task's run label and workflow id relate.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn chat_history(db: &Surreal<Db>, task: &str) -> Result<Vec<ChatMessage>> {
    let rows: Vec<ChatRow> = db
        .query(format!(
            "SELECT * FROM {CHAT_TABLE} WHERE task = $task ORDER BY at ASC"
        ))
        .bind(("task", task.to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(ChatRow::into_message).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::use_namespace;
    use crate::chat::append::append_chat;
    use crate::chat::model::ChatRole;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;

    async fn db() -> Surreal<Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn returns_both_sides_oldest_first() {
        let db = db().await;
        append_chat(&db, "wf", "t", ChatRole::User, "please fix the test", None)
            .await
            .unwrap();
        append_chat(&db, "wf", "t", ChatRole::Agent, "on it", Some(1))
            .await
            .unwrap();
        // A different task's message must not leak in.
        append_chat(&db, "wf", "other", ChatRole::User, "nope", None)
            .await
            .unwrap();

        let convo = chat_history(&db, "t").await.unwrap();
        assert_eq!(convo.len(), 2);
        assert_eq!(convo[0].role, ChatRole::User);
        assert_eq!(convo[0].text, "please fix the test");
        assert_eq!(convo[1].role, ChatRole::Agent);
    }

    #[tokio::test]
    async fn empty_for_unknown_task() {
        let db = db().await;
        assert!(chat_history(&db, "nope").await.unwrap().is_empty());
    }
}
