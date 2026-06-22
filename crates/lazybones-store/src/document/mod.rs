//! Documents: authored, branded markdown documents and the verbs that
//! create/read/list/update/delete them. Mirrors the [`skill`](crate::skill) CRUD
//! shape.
//!
//! A [`Document`] of [`kind`](DocKind) `Reference` is a reusable page (T&C) merged
//! into other documents via the [`attachment`](crate::attachment) seam
//! (`thing_kind="reference"`). Its uploads/context material are
//! [`source`](crate::source) rows (`thing_kind="source"`) — research behind the
//! doc that never renders. Optional GitHub publishing rides the [`DocRepo`] target.

mod create;
mod delete;
mod get;
mod list;
mod model;
mod row;
mod update;

pub use create::create_document;
pub use delete::delete_document;
pub use get::get_document;
pub use list::list_documents;
pub use model::{DocKind, DocRepo, Document};
pub use update::update_document;

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

    fn sample() -> Document {
        Document::new(
            "quote-1",
            "Quote for Acme",
            DocKind::Document,
            "# Quote\n\nLine items…",
            "2026-01-01T00:00:00Z",
        )
    }

    #[tokio::test]
    async fn create_get_list_delete_roundtrip() {
        let db = db().await;
        let created = create_document(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "quote-1");
        assert_eq!(created.kind, DocKind::Document);
        assert!(created.body.contains("Quote"));
        assert_eq!(created.project, None);

        let got = get_document(&db, "quote-1").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_documents(&db, None).await.unwrap();
        assert_eq!(all.len(), 1);

        assert!(delete_document(&db, "quote-1").await.unwrap());
        assert!(get_document(&db, "quote-1").await.unwrap().is_none());
        // Deleting again reports "did not exist".
        assert!(!delete_document(&db, "quote-1").await.unwrap());
    }

    #[tokio::test]
    async fn reference_kind_and_repo_roundtrip() {
        let db = db().await;
        let mut doc = Document::new(
            "tncs",
            "Terms & Conditions",
            DocKind::Reference,
            "Standard T&C…",
            "2026-01-01T00:00:00Z",
        )
        .with_repo(DocRepo {
            repo: "/repos/site".into(),
            base_branch: Some("main".into()),
            output_path: "docs/tncs.md".into(),
            ..DocRepo::default()
        });
        doc.branding_id = Some("default".into());
        create_document(&db, &doc).await.unwrap();

        let got = get_document(&db, "tncs").await.unwrap().unwrap();
        assert_eq!(got.kind, DocKind::Reference);
        assert_eq!(got.branding_id.as_deref(), Some("default"));
        let repo = got.repo.unwrap();
        assert_eq!(repo.repo, "/repos/site");
        assert_eq!(repo.output_path, "docs/tncs.md");
        assert_eq!(repo.base_branch.as_deref(), Some("main"));
        assert_eq!(repo.pr_url, None);
    }

    #[tokio::test]
    async fn create_duplicate_is_error() {
        let db = db().await;
        create_document(&db, &sample()).await.unwrap();
        let err = create_document(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::DocumentExists(_)));
    }

    #[tokio::test]
    async fn update_edits_fields_and_preserves_created_at() {
        let db = db().await;
        create_document(&db, &sample()).await.unwrap();

        let mut edited = Document::new(
            "quote-1",
            "Quote for Acme (rev 2)",
            DocKind::Document,
            "# Quote\n\nRevised line items.",
            "2026-02-02T00:00:00Z",
        );
        // Fill in the GitHub linkage that an action would persist over time.
        edited.repo = Some(DocRepo {
            repo: "/repos/site".into(),
            output_path: "docs/quote-1.md".into(),
            branch: Some("doc/quote-1".into()),
            pr_url: Some("https://github.com/x/y/pull/1".into()),
            ..DocRepo::default()
        });
        let updated = update_document(&db, &edited).await.unwrap();
        assert_eq!(updated.title, "Quote for Acme (rev 2)");
        assert_eq!(
            updated.repo.as_ref().unwrap().pr_url.as_deref(),
            Some("https://github.com/x/y/pull/1")
        );
        // The creation stamp is immutable; only `updated_at` moves.
        assert_eq!(updated.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(updated.updated_at, "2026-02-02T00:00:00Z");

        let got = get_document(&db, "quote-1").await.unwrap().unwrap();
        assert_eq!(got, updated);
    }

    #[tokio::test]
    async fn update_unknown_is_not_found() {
        let db = db().await;
        let err = update_document(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::DocumentNotFound(_)));
    }
}
