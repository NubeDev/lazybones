//! The hcom log: durable raw agent events (message/status/life) hcom observed,
//! keyed to the run/task that owns the agent (docs/hcom-logs-scope.md).
//!
//! The **fabric's** record, alongside the [`event`](crate::Event) run log (the
//! **brain's** record). Append is idempotent on `(run, hcom_id)`; reads come back
//! oldest-first, ordered by hcom's monotonic event id.

mod append;
mod history;
mod row;

pub use append::{NewHcomLogEntry, append_hcom_log};
pub use history::{HcomLogFilter, run_hcom_log};
pub use row::HcomLogEntry;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;

    use super::*;

    async fn db() -> surrealdb::Surreal<surrealdb::engine::local::Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    fn event(hcom_id: i64, kind: &str, text: &str) -> NewHcomLogEntry {
        NewHcomLogEntry {
            run: "run-1".into(),
            task: Some("auth".into()),
            agent: "kula".into(),
            tag: Some("auth".into()),
            hcom_id,
            kind: kind.into(),
            data: json!({ "text": text }),
            at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn append_then_read_oldest_first() {
        let db = db().await;
        append_hcom_log(&db, event(3, "message", "third")).await.unwrap();
        append_hcom_log(&db, event(1, "message", "first")).await.unwrap();
        append_hcom_log(&db, event(2, "status", "second")).await.unwrap();

        let all = run_hcom_log(&db, "run-1", &HcomLogFilter::default()).await.unwrap();
        let ids: Vec<i64> = all.iter().map(|e| e.hcom_id).collect();
        assert_eq!(ids, vec![1, 2, 3], "ordered by hcom_id ascending");
    }

    #[tokio::test]
    async fn append_is_idempotent_on_run_and_hcom_id() {
        let db = db().await;
        append_hcom_log(&db, event(1, "message", "one")).await.unwrap();
        // Re-ingest the same (run, hcom_id): a no-op, not a duplicate row.
        append_hcom_log(&db, event(1, "message", "one")).await.unwrap();

        let all = run_hcom_log(&db, "run-1", &HcomLogFilter::default()).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn filters_by_kind_task_and_after() {
        let db = db().await;
        append_hcom_log(&db, event(1, "message", "m")).await.unwrap();
        append_hcom_log(&db, event(2, "status", "s")).await.unwrap();
        let mut other = event(3, "message", "other-task");
        other.task = Some("ui".into());
        append_hcom_log(&db, other).await.unwrap();

        let only_messages = run_hcom_log(
            &db,
            "run-1",
            &HcomLogFilter { kind: Some("message".into()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(only_messages.len(), 2);

        let only_auth = run_hcom_log(
            &db,
            "run-1",
            &HcomLogFilter { task: Some("auth".into()), ..Default::default() },
        )
        .await
        .unwrap();
        assert_eq!(only_auth.len(), 2, "events 1 & 2 are tagged auth");

        let after_1 = run_hcom_log(
            &db,
            "run-1",
            &HcomLogFilter { after: Some(1), ..Default::default() },
        )
        .await
        .unwrap();
        let ids: Vec<i64> = after_1.iter().map(|e| e.hcom_id).collect();
        assert_eq!(ids, vec![2, 3]);
    }

    #[tokio::test]
    async fn stores_data_verbatim_and_keeps_at() {
        let db = db().await;
        let stored = append_hcom_log(&db, event(1, "message", "hello")).await.unwrap();
        assert_eq!(stored.data["text"], json!("hello"));
        assert_eq!(stored.at, "2026-01-01T00:00:00Z");
        assert_eq!(stored.agent, "kula");
        assert_eq!(stored.tag.as_deref(), Some("auth"));
    }
}
