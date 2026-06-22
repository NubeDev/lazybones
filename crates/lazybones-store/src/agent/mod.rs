//! The CRUD-able agent catalog: the model, its persisted row, the verbs that
//! create/read/update/list/delete entries, and the boot seed of 2026 defaults.

mod create;
mod delete;
mod get;
mod list;
mod model;
mod row;
mod seed;
mod update;

pub use create::create_agent;
pub use delete::delete_agent;
pub use get::get_agent;
pub use list::list_agents;
pub use model::{AgentCatalog, AgentCatalogEdit};
pub use seed::seed_default_agents;
pub use update::update_agent;

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

    fn sample() -> AgentCatalog {
        AgentCatalog::new(
            "claude",
            "Claude Code",
            "ANTHROPIC_API_KEY",
            "console.anthropic.com",
            vec!["claude-opus-4-8".into(), "claude-sonnet-4-6".into()],
            Some("claude-opus-4-8".into()),
            vec!["low".into(), "high".into()],
            Some("high".into()),
            "2026-01-01T00:00:00Z",
        )
    }

    #[tokio::test]
    async fn create_get_list_delete_roundtrip() {
        let db = db().await;
        let created = create_agent(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "claude");
        assert_eq!(created.models.len(), 2);
        assert_eq!(created.default_model.as_deref(), Some("claude-opus-4-8"));

        let got = get_agent(&db, "claude").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_agents(&db).await.unwrap();
        assert_eq!(all.len(), 1);

        assert!(delete_agent(&db, "claude").await.unwrap());
        assert!(get_agent(&db, "claude").await.unwrap().is_none());
        assert!(!delete_agent(&db, "claude").await.unwrap());
    }

    #[tokio::test]
    async fn create_duplicate_is_error() {
        let db = db().await;
        create_agent(&db, &sample()).await.unwrap();
        let err = create_agent(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::AgentExists(_)));
    }

    #[tokio::test]
    async fn update_overwrites_fields_and_preserves_created_at() {
        let db = db().await;
        create_agent(&db, &sample()).await.unwrap();

        let edit = AgentCatalogEdit {
            label: "Claude (renamed)".into(),
            env_var: "ANTHROPIC_API_KEY".into(),
            login_hint: "new hint".into(),
            models: vec!["claude-fable-5".into()],
            default_model: Some("claude-fable-5".into()),
            efforts: vec!["max".into()],
            default_effort: Some("max".into()),
        };
        let updated = update_agent(&db, "claude", edit, "2026-02-02T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(updated.label, "Claude (renamed)");
        assert_eq!(updated.models, vec!["claude-fable-5".to_owned()]);
        assert_eq!(updated.default_effort.as_deref(), Some("max"));
        // id + created_at preserved; updated_at bumped.
        assert_eq!(updated.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(updated.updated_at, "2026-02-02T00:00:00Z");
    }

    #[tokio::test]
    async fn update_missing_is_error() {
        let db = db().await;
        let edit = AgentCatalogEdit {
            label: "x".into(),
            env_var: "x".into(),
            login_hint: String::new(),
            models: vec![],
            default_model: None,
            efforts: vec![],
            default_effort: None,
        };
        let err = update_agent(&db, "ghost", edit, "2026-01-01T00:00:00Z")
            .await
            .unwrap_err();
        assert!(matches!(err, crate::StoreError::AgentNotFound(_)));
    }

    #[tokio::test]
    async fn seed_creates_defaults_then_is_idempotent_and_non_clobbering() {
        let db = db().await;

        // First seed populates the bundled catalog.
        let n = seed_default_agents(&db, "2026-01-01T00:00:00Z").await.unwrap();
        assert!(n >= 5, "expected the bundled defaults to seed, got {n}");
        let claude = get_agent(&db, "claude").await.unwrap().unwrap();
        assert!(claude.models.contains(&"claude-opus-4-8".to_owned()));

        // An operator edits one entry.
        let edit = AgentCatalogEdit {
            label: "MINE".into(),
            env_var: claude.env_var.clone(),
            login_hint: claude.login_hint.clone(),
            models: vec![],
            default_model: None,
            efforts: vec![],
            default_effort: None,
        };
        update_agent(&db, "claude", edit, "2026-03-03T00:00:00Z")
            .await
            .unwrap();

        // Re-seeding creates nothing new and leaves the edited entry untouched.
        let n2 = seed_default_agents(&db, "2026-04-04T00:00:00Z").await.unwrap();
        assert_eq!(n2, 0, "re-seed must not recreate existing ids");
        let after = get_agent(&db, "claude").await.unwrap().unwrap();
        assert_eq!(after.label, "MINE", "operator edit must survive re-seed");
    }
}
