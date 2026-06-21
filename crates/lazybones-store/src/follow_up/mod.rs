//! Follow-ups: durable "a human needs to act" notes, keyed to a run/task.
//!
//! When the scheduler hits a wall it can't clear on its own — an agent parked on
//! an interactive consent screen, a missing credential, a repeated spawn failure
//! — it files a follow-up instead of looping silently, and (for human-actionable
//! walls) blocks the task so it stops burning relaunches. Agents can file their
//! own via `POST /follow-ups`. Filing is idempotent on `(run, dedup_key)`: the
//! same wall bumps one row's `seen` gauge rather than spawning duplicates.

mod append;
mod history;
mod row;

pub use append::{NewFollowUpEntry, file_follow_up};
pub use history::{FollowUpFilter, resolve_follow_up, run_follow_ups};
pub use row::FollowUp;

#[cfg(test)]
mod tests {
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

    fn entry(dedup: &str, title: &str) -> NewFollowUpEntry {
        NewFollowUpEntry {
            run: "run-1".into(),
            task: Some("review".into()),
            dedup_key: dedup.into(),
            kind: "consent".into(),
            title: title.into(),
            detail: "do the thing".into(),
            actor: "scheduler".into(),
        }
    }

    #[tokio::test]
    async fn file_is_idempotent_and_bumps_seen() {
        let db = db().await;
        let first = file_follow_up(&db, entry("consent:review", "stuck")).await.unwrap();
        assert_eq!(first.seen, 1);
        assert_eq!(first.status, "open");

        let again = file_follow_up(&db, entry("consent:review", "still stuck")).await.unwrap();
        assert_eq!(again.seen, 2, "same dedup_key bumps seen");
        assert_eq!(again.title, "still stuck", "re-file refreshes the title");

        let all = run_follow_ups(&db, "run-1", &FollowUpFilter::default()).await.unwrap();
        assert_eq!(all.len(), 1, "one row, not two");
    }

    #[tokio::test]
    async fn resolve_then_refile_reopens() {
        let db = db().await;
        let filed = file_follow_up(&db, entry("spawn:review", "boom")).await.unwrap();
        let resolved = resolve_follow_up(&db, &filed.id).await.unwrap().unwrap();
        assert_eq!(resolved.status, "resolved");
        assert!(resolved.resolved_at.is_some());

        // The wall came back: re-filing re-opens the same row.
        let reopened = file_follow_up(&db, entry("spawn:review", "boom again")).await.unwrap();
        assert_eq!(reopened.status, "open");
        assert_eq!(reopened.resolved_at, None);
        assert_eq!(reopened.seen, 2);
    }

    #[tokio::test]
    async fn filters_by_status_and_task() {
        let db = db().await;
        file_follow_up(&db, entry("a", "one")).await.unwrap();
        let mut other = entry("b", "two");
        other.task = Some("pr".into());
        file_follow_up(&db, other).await.unwrap();
        let filed_c = file_follow_up(&db, entry("c", "three")).await.unwrap();
        resolve_follow_up(&db, &filed_c.id).await.unwrap();

        let open = run_follow_ups(
            &db,
            "run-1",
            &FollowUpFilter { status: Some("open".into()), task: None },
        )
        .await
        .unwrap();
        assert_eq!(open.len(), 2, "a and b are open; c is resolved");

        let pr = run_follow_ups(
            &db,
            "run-1",
            &FollowUpFilter { status: None, task: Some("pr".into()) },
        )
        .await
        .unwrap();
        assert_eq!(pr.len(), 1);
    }

    #[tokio::test]
    async fn resolve_unknown_id_is_none() {
        let db = db().await;
        let missing = resolve_follow_up(&db, "nope").await.unwrap();
        assert!(missing.is_none());
    }
}
