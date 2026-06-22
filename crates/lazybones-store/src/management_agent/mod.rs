//! The single-record Lazybones-Agent configuration: the model, its persisted
//! row, and the get/put verbs (`docs/agent/lazybones-agent-scope.md` §5).

mod get;
mod model;
mod put;
mod row;

pub use get::{
    get_management_agent, get_management_agent_resolved, get_management_agent_scoped,
};
pub use model::{
    ManagementAgentConfig, ManagementAgentScope, PermissionProfile, SessionMode,
};
pub use put::{
    delete_management_agent_scoped, put_management_agent, put_management_agent_scoped,
};

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

    #[tokio::test]
    async fn absent_until_written() {
        let db = db().await;
        assert!(get_management_agent(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn workflow_override_resolves_then_falls_back_to_global() {
        let db = db().await;
        let global = ManagementAgentConfig {
            tool: "claude".into(),
            permission_profile: PermissionProfile::Author,
            ..ManagementAgentConfig::default()
        };
        put_management_agent(&db, &global).await.unwrap();

        let wf = ManagementAgentScope::Workflow("wf-1".into());

        // No override yet: resolving the workflow scope yields the global config.
        let resolved = get_management_agent_resolved(&db, &wf).await.unwrap().unwrap();
        assert_eq!(resolved.permission_profile, PermissionProfile::Author);

        // Add a workflow override: resolution now prefers it.
        let override_cfg = ManagementAgentConfig {
            permission_profile: PermissionProfile::ReadOnly,
            ..global.clone()
        };
        put_management_agent_scoped(&db, &wf, &override_cfg).await.unwrap();
        let resolved = get_management_agent_resolved(&db, &wf).await.unwrap().unwrap();
        assert_eq!(resolved.permission_profile, PermissionProfile::ReadOnly);

        // The global record is untouched by the override.
        let g = get_management_agent(&db).await.unwrap().unwrap();
        assert_eq!(g.permission_profile, PermissionProfile::Author);

        // Deleting the override reverts the workflow to the global default.
        assert!(delete_management_agent_scoped(&db, &wf).await.unwrap());
        let resolved = get_management_agent_resolved(&db, &wf).await.unwrap().unwrap();
        assert_eq!(resolved.permission_profile, PermissionProfile::Author);
    }

    #[tokio::test]
    async fn put_then_get_roundtrip_and_overwrite() {
        let db = db().await;
        let cfg = ManagementAgentConfig {
            tool: "claude".into(),
            model: Some("claude-opus-4-8".into()),
            effort: Some("high".into()),
            permission_profile: PermissionProfile::Author,
            session_mode: SessionMode::PerConversation,
            enabled_skills: vec!["lazybones-add-workflow".into()],
            permission_flags: vec!["--dangerously-skip-permissions".into()],
            updated_at: "2026-06-21T00:00:00Z".into(),
        };
        let written = put_management_agent(&db, &cfg).await.unwrap();
        assert_eq!(written, cfg);

        let got = get_management_agent(&db).await.unwrap().unwrap();
        assert_eq!(got, cfg);

        // Overwriting replaces the single record in place.
        let cfg2 = ManagementAgentConfig {
            permission_profile: PermissionProfile::ReadOnly,
            enabled_skills: vec![],
            ..cfg
        };
        put_management_agent(&db, &cfg2).await.unwrap();
        let got2 = get_management_agent(&db).await.unwrap().unwrap();
        assert_eq!(got2.permission_profile, PermissionProfile::ReadOnly);
        assert!(got2.enabled_skills.is_empty());
    }
}
