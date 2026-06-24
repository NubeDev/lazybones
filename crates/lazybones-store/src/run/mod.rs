//! Workflows (stored in the `run` table): the workspace-bound model, its row, the
//! verbs that create/read/list/stop/resume/start them, the `list_run_tasks`
//! query, and the pure `derived_state` function (run state is derived, never
//! stored).

mod create;
mod cursor;
mod delete;
mod derived;
mod get;
mod list;
mod model;
mod resume;
mod row;
mod start;
mod stop;
mod update;

pub use create::create_run;
pub use cursor::advance_hcom_cursor;
pub use delete::delete_run;
pub use derived::{RunState, derived_state};
pub use get::get_run;
pub use list::{list_run_tasks, list_runs};
pub use model::{Lifecycle, MergeMode, Run, Workspace};
pub use resume::resume_run;
pub use start::{clear_started, mark_started, set_pr_url};
pub use stop::stop_run;
pub use update::update_workspace;

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
                worktree_name: None,
                tool: None,
                model: None,
                effort: None,
                gate: None,
                merge: None,
                auto_pr: None,
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
    async fn auto_pr_and_pr_url_roundtrip() {
        let db = db().await;
        let mut run = sample();
        run.workspace.auto_pr = Some(true);
        create_run(&db, &run).await.unwrap();
        // The opt-in flag survives a store roundtrip; pr_url starts unset.
        let got = get_run(&db, "workflow-1").await.unwrap().unwrap();
        assert_eq!(got.workspace.auto_pr, Some(true));
        assert_eq!(got.pr_url, None);

        // Recording the opened PR is the idempotency guard; it persists.
        let after = set_pr_url(&db, "workflow-1", "https://github.com/x/y/pull/1").await.unwrap();
        assert_eq!(after.pr_url.as_deref(), Some("https://github.com/x/y/pull/1"));
        assert_eq!(
            get_run(&db, "workflow-1").await.unwrap().unwrap().pr_url.as_deref(),
            Some("https://github.com/x/y/pull/1")
        );

        // A restart (clear_started) drops the PR url so completion opens a fresh one.
        let cleared = clear_started(&db, "workflow-1").await.unwrap();
        assert_eq!(cleared.pr_url, None);
    }

    #[tokio::test]
    async fn update_workspace_overwrites_defaults_but_keeps_repo() {
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();

        let edited = update_workspace(
            &db,
            "workflow-1",
            Workspace {
                // A caller might pass a different repo; it must be ignored.
                repo: "/some/other/repo".into(),
                base_branch: Some("dev".into()),
                branch_prefix: None,
                worktree_mode: WorktreeMode::Reuse,
                worktree_name: None,
                tool: Some("claude".into()),
                model: Some("claude-opus-4-8".into()),
                effort: Some("high".into()),
                gate: None,
                merge: Some(MergeMode::Merge),
                auto_pr: None,
            },
        )
        .await
        .unwrap();

        // The repo is preserved; everything else is overwritten.
        assert_eq!(edited.workspace.repo, "/repo/abc");
        assert_eq!(edited.workspace.tool.as_deref(), Some("claude"));
        assert_eq!(edited.workspace.model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(edited.workspace.effort.as_deref(), Some("high"));
        assert_eq!(edited.workspace.worktree_mode, WorktreeMode::Reuse);
        assert_eq!(edited.workspace.merge, Some(MergeMode::Merge));

        // Persisted across a read.
        let got = get_run(&db, "workflow-1").await.unwrap().unwrap();
        assert_eq!(got, edited);
    }

    #[tokio::test]
    async fn update_workspace_missing_run_is_error() {
        let db = db().await;
        let err = update_workspace(&db, "nope", sample().workspace)
            .await
            .unwrap_err();
        assert!(matches!(err, crate::StoreError::RunNotFound(_)));
    }

    #[tokio::test]
    async fn stop_and_resume_flip_lifecycle() {
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();
        let stopped = stop_run(&db, "workflow-1").await.unwrap();
        assert_eq!(stopped.lifecycle, Lifecycle::Stopped);
        // Resume is the reverse — a stopped run is never terminal.
        let resumed = resume_run(&db, "workflow-1").await.unwrap();
        assert_eq!(resumed.lifecycle, Lifecycle::Active);
    }

    #[tokio::test]
    async fn newly_ready_excludes_stopped_runs_tasks() {
        use crate::task::newly_ready;
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();

        // A no-dep pending task owned by workflow-1 — ready while the run is live.
        let mut t = Task::seed("t1", "workflow-1", "T1", "spec", vec![], vec![], None);
        t.run_id = Some("workflow-1".into());
        create_task(&db, &t).await.unwrap();
        assert_eq!(newly_ready(&db, &[]).await.unwrap(), vec!["t1".to_owned()]);

        // Once the parent run is stopped, the promote query drops its task.
        stop_run(&db, "workflow-1").await.unwrap();
        let stopped = vec!["workflow-1".to_owned()];
        assert!(newly_ready(&db, &stopped).await.unwrap().is_empty());
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
    async fn clear_started_un_activates_and_restart_can_start_again() {
        let db = db().await;
        create_run(&db, &sample()).await.unwrap();
        mark_started(&db, "workflow-1", "2026-02-02T00:00:00Z").await.unwrap();

        // Clearing drops the stamp (the scheduler then skips the run) and forces
        // lifecycle Active so even a stopped run becomes startable again.
        let cleared = clear_started(&db, "workflow-1").await.unwrap();
        assert_eq!(cleared.started_at, None, "started_at cleared by restart");
        assert_eq!(cleared.lifecycle, Lifecycle::Active);

        // A subsequent start re-stamps with the new time (no longer idempotent to
        // the old value, because the field was cleared).
        let restarted = mark_started(&db, "workflow-1", "2026-04-04T00:00:00Z").await.unwrap();
        assert_eq!(restarted.started_at.as_deref(), Some("2026-04-04T00:00:00Z"));
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
