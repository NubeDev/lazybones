//! Projects: the team graph's only new noun, its persisted row, the full
//! create/get/list/update/delete verbs, and the [`under`](under) containment
//! helper (placement into a team + the downward traversal). Mirrors the
//! [`skill`](crate::skill) module shape. Assignment of a project's workflows to an
//! edge lives on the separate `scoped_to` relation, not here.

mod create;
mod delete;
mod get;
mod list;
mod model;
mod row;
mod under;
mod update;

pub use create::create_project;
pub use delete::delete_project;
pub use get::get_project;
pub use list::list_projects;
pub use model::{Project, ProjectStatus};
pub use under::{place_project_under_team, team_projects};
pub use update::update_project;

#[cfg(test)]
mod tests {
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;
    use crate::team::{Team, create_team};

    use super::*;

    async fn db() -> surrealdb::Surreal<surrealdb::engine::local::Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    fn sample() -> Project {
        Project::new("apollo", "Apollo", "2026-01-01T00:00:00Z")
            .with_team("platform")
            .with_repos(vec!["repo:app".into(), "repo:infra".into()])
    }

    #[tokio::test]
    async fn create_get_list_update_delete_roundtrip() {
        let db = db().await;
        let created = create_project(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "apollo");
        assert_eq!(created.status, ProjectStatus::Active);
        assert_eq!(created.team.as_deref(), Some("platform"));
        assert_eq!(created.repos, vec!["repo:app".to_owned(), "repo:infra".to_owned()]);

        let got = get_project(&db, "apollo").await.unwrap().unwrap();
        assert_eq!(got, created);

        // The denormalized `team` column narrows the list.
        assert_eq!(list_projects(&db, Some("platform")).await.unwrap().len(), 1);
        assert!(list_projects(&db, Some("growth")).await.unwrap().is_empty());
        assert_eq!(list_projects(&db, None).await.unwrap().len(), 1);

        assert!(delete_project(&db, "apollo").await.unwrap());
        assert!(get_project(&db, "apollo").await.unwrap().is_none());
        assert!(!delete_project(&db, "apollo").await.unwrap());
    }

    #[tokio::test]
    async fn teamless_project_reads_back_none() {
        let db = db().await;
        create_project(&db, &Project::new("solo", "Solo", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        let got = get_project(&db, "solo").await.unwrap().unwrap();
        assert_eq!(got.team, None);
        assert!(got.repos.is_empty());
    }

    #[tokio::test]
    async fn create_duplicate_is_error() {
        let db = db().await;
        create_project(&db, &sample()).await.unwrap();
        let err = create_project(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::ProjectExists(_)));
    }

    #[tokio::test]
    async fn update_edits_fields_and_preserves_created_at() {
        let db = db().await;
        create_project(&db, &sample()).await.unwrap();

        let mut edited = Project::new("apollo", "Apollo (archived)", "2026-02-02T00:00:00Z")
            .with_team("platform");
        edited.status = ProjectStatus::Archived;
        let updated = update_project(&db, &edited).await.unwrap();
        assert_eq!(updated.title, "Apollo (archived)");
        assert_eq!(updated.status, ProjectStatus::Archived);
        // The creation stamp is immutable; only `updated_at` moves.
        assert_eq!(updated.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(updated.updated_at, "2026-02-02T00:00:00Z");

        let got = get_project(&db, "apollo").await.unwrap().unwrap();
        assert_eq!(got, updated);
    }

    #[tokio::test]
    async fn update_unknown_is_not_found() {
        let db = db().await;
        let err = update_project(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::ProjectNotFound(_)));
    }

    #[tokio::test]
    async fn containment_traversal_returns_team_projects() {
        let db = db().await;
        create_team(&db, &Team::new("platform", "Platform", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_team(&db, &Team::new("growth", "Growth", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_project(&db, &Project::new("apollo", "Apollo", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_project(&db, &Project::new("gemini", "Gemini", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        create_project(&db, &Project::new("ranger", "Ranger", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();

        place_project_under_team(&db, "apollo", "platform").await.unwrap();
        place_project_under_team(&db, "gemini", "platform").await.unwrap();
        place_project_under_team(&db, "ranger", "growth").await.unwrap();

        let mut children: Vec<String> = team_projects(&db, "platform")
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        children.sort();
        assert_eq!(children, vec!["apollo".to_owned(), "gemini".to_owned()]);
    }

    #[tokio::test]
    async fn place_under_missing_team_is_not_found() {
        let db = db().await;
        create_project(&db, &Project::new("apollo", "Apollo", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        let err = place_project_under_team(&db, "apollo", "ghost").await.unwrap_err();
        assert!(matches!(err, crate::StoreError::TeamNotFound(_)));
    }
}
