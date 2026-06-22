//! Sources: a document's uploads / context material (links and uploaded files)
//! and the verbs that create/read/list/update/delete them. Mirrors the
//! [`skill`](crate::skill) CRUD shape (per-document listing instead of a global
//! list, and no install-wide id collision since ids are caller-minted).
//!
//! Sources are research material **behind** a [`Document`](crate::Document) that
//! never renders (unlike a [`Reference`](crate::DocKind)). File sources reuse the
//! content-addressed [`BlobStore`](crate::BlobStore) + sha256 dedup via an
//! [`Asset`](crate::Asset); PDF uploads get plain text pulled into
//! `extracted_text` by [`extract_pdf_text`].

mod create;
mod delete;
mod extract;
mod get;
mod list;
mod model;
mod row;
mod update;

pub use create::create_source;
pub use delete::delete_source;
pub use extract::extract_pdf_text;
pub use get::get_source;
pub use list::list_sources;
pub use model::{Source, SourceKind};
pub use update::update_source;

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
    async fn create_get_list_delete_roundtrip() {
        let db = db().await;
        let link = Source::link(
            "src-1",
            "doc-1",
            "https://example.com/spec",
            "The spec",
            "2026-01-01T00:00:00Z",
        );
        let created = create_source(&db, &link).await.unwrap();
        assert_eq!(created.id, "src-1");
        assert_eq!(created.kind, SourceKind::Link);
        assert_eq!(created.url.as_deref(), Some("https://example.com/spec"));
        assert_eq!(created.project, None);

        let got = get_source(&db, "src-1").await.unwrap().unwrap();
        assert_eq!(got, created);

        let listed = list_sources(&db, "doc-1").await.unwrap();
        assert_eq!(listed.len(), 1);

        assert!(delete_source(&db, "src-1").await.unwrap());
        assert!(get_source(&db, "src-1").await.unwrap().is_none());
        // Deleting again reports "did not exist".
        assert!(!delete_source(&db, "src-1").await.unwrap());
    }

    #[tokio::test]
    async fn list_is_scoped_to_one_document() {
        let db = db().await;
        create_source(
            &db,
            &Source::link("a", "doc-1", "https://x", "x", "2026-01-01T00:00:00Z"),
        )
        .await
        .unwrap();
        create_source(
            &db,
            &Source::link("b", "doc-2", "https://y", "y", "2026-01-01T00:00:00Z"),
        )
        .await
        .unwrap();

        let doc1 = list_sources(&db, "doc-1").await.unwrap();
        assert_eq!(doc1.len(), 1);
        assert_eq!(doc1[0].id, "a");
    }

    #[tokio::test]
    async fn file_source_with_extracted_text_roundtrips() {
        let db = db().await;
        let file = Source::file(
            "src-pdf",
            "doc-1",
            "asset-1",
            "Reference PDF",
            "application/pdf",
            "2026-01-01T00:00:00Z",
        )
        .with_extracted_text("hello from the pdf");
        create_source(&db, &file).await.unwrap();

        let got = get_source(&db, "src-pdf").await.unwrap().unwrap();
        assert_eq!(got.kind, SourceKind::File);
        assert_eq!(got.asset_id.as_deref(), Some("asset-1"));
        assert_eq!(got.content_type, "application/pdf");
        assert_eq!(got.extracted_text.as_deref(), Some("hello from the pdf"));
    }

    #[tokio::test]
    async fn update_backfills_extracted_text_and_preserves_created_at() {
        let db = db().await;
        let file = Source::file(
            "src-pdf",
            "doc-1",
            "asset-1",
            "Reference PDF",
            "application/pdf",
            "2026-01-01T00:00:00Z",
        );
        create_source(&db, &file).await.unwrap();

        let edited = file.clone().with_extracted_text("extracted later");
        let updated = update_source(&db, &edited).await.unwrap();
        assert_eq!(updated.extracted_text.as_deref(), Some("extracted later"));
        // The creation stamp is immutable.
        assert_eq!(updated.created_at, "2026-01-01T00:00:00Z");

        let got = get_source(&db, "src-pdf").await.unwrap().unwrap();
        assert_eq!(got, updated);
    }

    #[tokio::test]
    async fn update_unknown_is_not_found() {
        let db = db().await;
        let ghost = Source::link("ghost", "doc-1", "https://x", "x", "2026-01-01T00:00:00Z");
        let err = update_source(&db, &ghost).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::SourceNotFound(_)));
    }

    #[test]
    fn extract_pdf_text_on_non_pdf_is_none() {
        // A non-PDF (or malformed) upload degrades to `None`, never a panic.
        assert!(extract_pdf_text(b"this is not a pdf at all").is_none());
        assert!(extract_pdf_text(&[]).is_none());
    }
}
