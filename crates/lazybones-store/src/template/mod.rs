//! Task templates: the reusable recipe model, its persisted row, the verbs that
//! create/read/list/delete them, and the `instantiate` helper (template → task).

mod create;
mod delete;
mod get;
mod instantiate;
mod list;
mod model;
mod row;

pub use create::create_template;
pub use delete::delete_template;
pub use get::get_template;
pub use instantiate::instantiate;
pub use list::list_templates;
pub use model::Template;

#[cfg(test)]
mod tests {
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;
    use crate::task::WorktreeMode;

    use super::*;

    async fn db() -> surrealdb::Surreal<surrealdb::engine::local::Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    fn sample() -> Template {
        Template::new(
            "open-pr",
            "Open a PR",
            "Open a pull request for the branch",
            "Implement the change and open a PR.",
            Some("claude".into()),
            None,
            None,
            None,
            "2026-01-01T00:00:00Z",
        )
    }

    #[tokio::test]
    async fn create_get_list_delete_roundtrip() {
        let db = db().await;
        let created = create_template(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "open-pr");
        assert_eq!(created.default_tool.as_deref(), Some("claude"));

        let got = get_template(&db, "open-pr").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_templates(&db).await.unwrap();
        assert_eq!(all.len(), 1);

        assert!(delete_template(&db, "open-pr").await.unwrap());
        assert!(get_template(&db, "open-pr").await.unwrap().is_none());
        // Deleting again reports "did not exist".
        assert!(!delete_template(&db, "open-pr").await.unwrap());
    }

    #[tokio::test]
    async fn create_duplicate_is_error() {
        let db = db().await;
        create_template(&db, &sample()).await.unwrap();
        let err = create_template(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::TemplateExists(_)));
    }

    #[tokio::test]
    async fn default_worktree_mode_roundtrips() {
        let db = db().await;
        let mut t = sample();
        t.default_worktree_mode = Some(WorktreeMode::Branch);
        create_template(&db, &t).await.unwrap();
        let got = get_template(&db, "open-pr").await.unwrap().unwrap();
        assert_eq!(got.default_worktree_mode, Some(WorktreeMode::Branch));
    }

    #[test]
    fn instantiate_carries_provenance_and_mode() {
        let mut t = sample();
        t.default_worktree_mode = Some(WorktreeMode::Reuse);
        let task = instantiate(&t, "new-api", "New API", "run", "wf-1", vec!["scaffold".into()]);
        assert_eq!(task.id, "new-api");
        assert_eq!(task.run_id.as_deref(), Some("wf-1"));
        assert_eq!(task.template_id.as_deref(), Some("open-pr"));
        assert_eq!(task.spec, t.spec_template);
        assert_eq!(task.tool.as_deref(), Some("claude"));
        assert_eq!(task.worktree_mode_override, Some(WorktreeMode::Reuse));
        assert_eq!(task.deps, vec!["scaffold".to_owned()]);
        assert_eq!(task.status, crate::Status::Pending);
    }
}
