//! Skills: the reusable agent-instruction model, its persisted row, and the verbs
//! that create/read/list/update/delete them. Mirrors the [`template`](crate::template)
//! CRUD shape. Attaching a skill to another entity lives in the generic
//! [`attachment`](crate::attachment) seam, not here.

mod create;
mod delete;
mod get;
mod list;
mod model;
mod row;
mod seed;
mod update;

pub use create::create_skill;
pub use delete::delete_skill;
pub use get::get_skill;
pub use list::list_skills;
pub use model::Skill;
pub use seed::seed_default_skills;
pub use update::update_skill;

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

    fn sample() -> Skill {
        Skill::new(
            "code-review-rust",
            "Rust code review",
            "How to review Rust changes",
            "Check for unwrap in non-test code, missing error context, …",
            "2026-01-01T00:00:00Z",
        )
    }

    #[tokio::test]
    async fn create_get_list_delete_roundtrip() {
        let db = db().await;
        let created = create_skill(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "code-review-rust");
        assert!(created.body.contains("unwrap"));

        let got = get_skill(&db, "code-review-rust").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_skills(&db).await.unwrap();
        assert_eq!(all.len(), 1);

        assert!(delete_skill(&db, "code-review-rust").await.unwrap());
        assert!(get_skill(&db, "code-review-rust").await.unwrap().is_none());
        // Deleting again reports "did not exist".
        assert!(!delete_skill(&db, "code-review-rust").await.unwrap());
    }

    #[tokio::test]
    async fn create_duplicate_is_error() {
        let db = db().await;
        create_skill(&db, &sample()).await.unwrap();
        let err = create_skill(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::SkillExists(_)));
    }

    #[tokio::test]
    async fn update_edits_fields_and_preserves_created_at() {
        let db = db().await;
        create_skill(&db, &sample()).await.unwrap();

        let edited = Skill::new(
            "code-review-rust",
            "Rust review (revised)",
            "A clearer description",
            "New instructions.",
            "2026-02-02T00:00:00Z",
        );
        let updated = update_skill(&db, &edited).await.unwrap();
        assert_eq!(updated.title, "Rust review (revised)");
        assert_eq!(updated.body, "New instructions.");
        // The creation stamp is immutable; only `updated_at` moves.
        assert_eq!(updated.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(updated.updated_at, "2026-02-02T00:00:00Z");

        let got = get_skill(&db, "code-review-rust").await.unwrap().unwrap();
        assert_eq!(got, updated);
    }

    #[tokio::test]
    async fn update_unknown_is_not_found() {
        let db = db().await;
        let err = update_skill(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::SkillNotFound(_)));
    }

    #[tokio::test]
    async fn seed_creates_demos_then_is_idempotent_and_non_clobbering() {
        let db = db().await;
        // First seed populates the demo catalogue.
        let n = seed_default_skills(&db, "2026-01-01T00:00:00Z").await.unwrap();
        assert!(n >= 3, "expected the bundled demos to seed, got {n}");
        let seeded = list_skills(&db).await.unwrap();
        assert_eq!(seeded.len(), n);

        // An operator edits one of the seeded skills.
        let mut edited = get_skill(&db, "code-review-rust").await.unwrap().unwrap();
        edited.title = "MINE".into();
        update_skill(&db, &edited).await.unwrap();

        // Re-seeding creates nothing new and leaves the edit intact.
        let n2 = seed_default_skills(&db, "2026-04-04T00:00:00Z").await.unwrap();
        assert_eq!(n2, 0, "re-seed must not recreate existing ids");
        let after = get_skill(&db, "code-review-rust").await.unwrap().unwrap();
        assert_eq!(after.title, "MINE", "operator edit must survive re-seed");
    }
}
