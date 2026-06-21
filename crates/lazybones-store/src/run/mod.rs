//! Workflows (stored in the `run` table): the workspace-bound model, its row, the
//! verbs that create/read/list/cancel/start them, the `list_run_tasks` query, and
//! the pure `derived_state` function (run state is derived, never stored).

mod cancel;
mod create;
mod cursor;
mod delete;
mod derived;
mod get;
mod list;
mod model;
mod row;
mod start;

pub use cancel::cancel_run;
pub use create::create_run;
pub use cursor::advance_hcom_cursor;
pub use delete::delete_run;
pub use derived::{RunState, derived_state};
pub use get::get_run;
pub use list::{list_run_tasks, list_runs};
pub use model::{Lifecycle, MergeMode, Run, Workspace};
pub use start::mark_started;

#[cfg(test)]
mod tests {
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;
    use crate::task::{Task, WorktreeMode};
    use crate::task::create::create_task;

    use super::*;

    async fn db() -> surrealdb::Surreal<surrealdb::engine::local::Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    fn sample() -> Run {
        Run::new(
            "workflow-1",
            "WF 1",
            Workspace {
                repo: "/repo/abc".into(),
                base_branch: None,
                branch_prefix: None,
                worktree_mode: WorktreeMode::New,
                tool: None,
                model: None,
                effort: None,
                gate: None,
                merge: None,
            },
            "2026-01-01T00:00:00Z",
        )
    }

    #[tokio::test]
    async fn create_get_list_roundtrip() {
        let db = db().await;
        let created = create_run(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "workflow-1");
        assert_eq!(created.lifecycle, Lifecycle::Active);

        let got = get_run(&db, "workflow-1").await.unwrap().unwrap();
        assert_eq!(got, created);

        assert_eq!(list_runs(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_duplicate_is_error() {
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();
        let err = create_run(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::RunExists(_)));
    }

    #[tokio::test]
    async fn workspace_overrides_roundtrip() {
        let db = db().await;
        let mut run = sample();
        run.workspace.base_branch = Some("dev".into());
        run.workspace.branch_prefix = Some("wf/".into());
        run.workspace.worktree_mode = WorktreeMode::Reuse;
        run.workspace.merge = Some(MergeMode::Merge);
        create_run(&db, &run).await.unwrap();
        let got = get_run(&db, "workflow-1").await.unwrap().unwrap();
        assert_eq!(got.workspace.base_branch.as_deref(), Some("dev"));
        assert_eq!(got.workspace.branch_prefix.as_deref(), Some("wf/"));
        assert_eq!(got.workspace.worktree_mode, WorktreeMode::Reuse);
        // The per-workflow merge strategy survives a store roundtrip.
        assert_eq!(got.workspace.merge, Some(MergeMode::Merge));
        // An absent merge reads back as None (inherit the global).
        assert_eq!(sample().workspace.merge, None);
    }

    #[tokio::test]
    async fn cancel_sets_lifecycle() {
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();
        let cancelled = cancel_run(&db, "workflow-1").await.unwrap();
        assert_eq!(cancelled.lifecycle, Lifecycle::Cancelled);
    }

    #[tokio::test]
    async fn start_stamps_once() {
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();
        let r1 = mark_started(&db, "workflow-1", "2026-02-02T00:00:00Z").await.unwrap();
        assert_eq!(r1.started_at.as_deref(), Some("2026-02-02T00:00:00Z"));
        // Idempotent: a second start keeps the first stamp.
        let r2 = mark_started(&db, "workflow-1", "2026-03-03T00:00:00Z").await.unwrap();
        assert_eq!(r2.started_at.as_deref(), Some("2026-02-02T00:00:00Z"));
    }

    #[tokio::test]
    async fn hcom_cursor_advances_monotonically() {
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();
        // Starts unset.
        assert_eq!(get_run(&db, "workflow-1").await.unwrap().unwrap().hcom_log_cursor, None);

        let r = advance_hcom_cursor(&db, "workflow-1", 5).await.unwrap();
        assert_eq!(r.hcom_log_cursor, Some(5));
        // A lower value never moves it backwards.
        let r = advance_hcom_cursor(&db, "workflow-1", 3).await.unwrap();
        assert_eq!(r.hcom_log_cursor, Some(5));
        // A higher value advances it.
        let r = advance_hcom_cursor(&db, "workflow-1", 9).await.unwrap();
        assert_eq!(r.hcom_log_cursor, Some(9));
    }

    #[tokio::test]
    async fn delete_cascades_tasks_and_spares_standalone() {
        use crate::task::get_task;

        let db = db().await;
        create_run(&db, &sample()).await.unwrap();
        let mut a = Task::seed("a", "r", "A", "s", vec![], vec![], None);
        a.run_id = Some("workflow-1".into());
        let standalone = Task::seed("c", "r", "C", "s", vec![], vec![], None);
        create_task(&db, &a).await.unwrap();
        create_task(&db, &standalone).await.unwrap();

        let existed = delete_run(&db, "workflow-1").await.unwrap();
        assert!(existed);
        // The run is gone, its task cascaded, the standalone task untouched.
        assert!(get_run(&db, "workflow-1").await.unwrap().is_none());
        assert!(get_task(&db, "a").await.unwrap().is_none());
        assert!(get_task(&db, "c").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn delete_missing_run_is_false() {
        let db = db().await;
        assert!(!delete_run(&db, "nope").await.unwrap());
    }

    #[tokio::test]
    async fn list_run_tasks_keys_off_run_id() {
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();
        // Two tasks linked to workflow-1, one standalone.
        let mut a = Task::seed("a", "r", "A", "s", vec![], vec![], None);
        a.run_id = Some("workflow-1".into());
        let mut b = Task::seed("b", "r", "B", "s", vec![], vec![], None);
        b.run_id = Some("workflow-1".into());
        let standalone = Task::seed("c", "r", "C", "s", vec![], vec![], None);
        create_task(&db, &a).await.unwrap();
        create_task(&db, &b).await.unwrap();
        create_task(&db, &standalone).await.unwrap();

        let mut tasks = list_run_tasks(&db, "workflow-1").await.unwrap();
        tasks.sort_by(|x, y| x.id.cmp(&y.id));
        let ids: Vec<_> = tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }
}
