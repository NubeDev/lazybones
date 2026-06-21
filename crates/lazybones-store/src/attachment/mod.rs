//! Generic attachments: link any owner entity to any polymorphic "thing".
//!
//! An attachment is a single row `(owner_kind, owner_id, thing_kind, thing_id,
//! created_at)`. Both ends are `(kind, id)` strings, so the same mechanism that
//! attaches a [`skill`](crate::skill) to a [`template`](crate::template) today can
//! attach a different thing-kind to a different owner tomorrow without a schema
//! change — there is no skill- or template-specific foreign key.
//!
//! ## Design choices
//! - **Idempotent attach.** A UNIQUE index on
//!   `(owner_kind, owner_id, thing_kind, thing_id)` (see
//!   [`init_schema`](crate::init_schema)) makes re-attaching the same link a
//!   no-op; [`attach`] returns the existing row.
//! - **No hard FK on the thing.** Because the thing is polymorphic and SurrealDB
//!   is SCHEMALESS, these verbs do **not** validate that the attached thing
//!   exists. A dangling attachment (e.g. its skill was deleted) is tolerated and
//!   simply reads as a thing-id whose target no longer resolves. Callers that
//!   care about owner existence enforce it themselves (the template routes 404 a
//!   missing owner before calling these verbs).

mod row;
mod verbs;

pub use row::Attachment;
pub use verbs::{attach, detach, list_attachments};

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
    async fn attach_is_idempotent() {
        let db = db().await;
        let first = attach(&db, "template", "open-pr", "skill", "code-review")
            .await
            .unwrap();
        let again = attach(&db, "template", "open-pr", "skill", "code-review")
            .await
            .unwrap();
        // Same link → same row, not a duplicate.
        assert_eq!(first.id, again.id);

        let all = list_attachments(&db, "template", "open-pr", None).await.unwrap();
        assert_eq!(all.len(), 1, "one row, not two");
    }

    #[tokio::test]
    async fn detach_removes() {
        let db = db().await;
        attach(&db, "template", "open-pr", "skill", "code-review").await.unwrap();

        assert!(detach(&db, "template", "open-pr", "skill", "code-review").await.unwrap());
        let all = list_attachments(&db, "template", "open-pr", None).await.unwrap();
        assert!(all.is_empty());
        // Detaching again reports "did not exist".
        assert!(!detach(&db, "template", "open-pr", "skill", "code-review").await.unwrap());
    }

    #[tokio::test]
    async fn list_filters_by_thing_kind() {
        let db = db().await;
        attach(&db, "template", "open-pr", "skill", "code-review").await.unwrap();
        attach(&db, "template", "open-pr", "note", "n-1").await.unwrap();

        let skills = list_attachments(&db, "template", "open-pr", Some("skill"))
            .await
            .unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].thing_kind, "skill");

        let all = list_attachments(&db, "template", "open-pr", None).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn missing_owner_behaves_as_empty() {
        let db = db().await;
        // The store verbs don't validate the owner — listing an owner with no
        // attachments is simply empty; detaching from it is simply false. (Route
        // handlers add the owner-exists 404 on top of this.)
        let all = list_attachments(&db, "template", "ghost", None).await.unwrap();
        assert!(all.is_empty());
        assert!(!detach(&db, "template", "ghost", "skill", "x").await.unwrap());
    }
}
