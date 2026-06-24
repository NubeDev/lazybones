//! Pages: the ordered sections of a [`Document`](crate::Document) (book) and the
//! verbs that create/read/list/update/delete them. Mirrors the
//! [`source`](crate::source) child-of-document CRUD shape (per-document listing,
//! caller-minted ids, plain insert).
//!
//! A document's rendered output is its pages assembled in
//! [`position`](Page::position) order — each page is a page-break boundary in the
//! exported PDF. Order is fractional, so reordering or inserting a page is a
//! single-row [`update_page`] of its position (see [`position_between`] /
//! [`append_position`]).

mod create;
mod delete;
mod get;
mod list;
mod model;
mod row;
mod update;

pub use create::create_page;
pub use delete::delete_page;
pub use get::get_page;
pub use list::list_pages;
pub use model::{Page, append_position, position_between};
pub use update::update_page;

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
        let page = Page::new("pg-1", "doc-1", "Intro", "# Hello", 1.0, "2026-01-01T00:00:00Z");
        let created = create_page(&db, &page).await.unwrap();
        assert_eq!(created.id, "pg-1");
        assert_eq!(created.document, "doc-1");
        assert_eq!(created.position, 1.0);
        assert_eq!(created.project, None);

        let got = get_page(&db, "pg-1").await.unwrap().unwrap();
        assert_eq!(got, created);

        let listed = list_pages(&db, "doc-1").await.unwrap();
        assert_eq!(listed.len(), 1);

        assert!(delete_page(&db, "pg-1").await.unwrap());
        assert!(get_page(&db, "pg-1").await.unwrap().is_none());
        // Deleting again reports "did not exist".
        assert!(!delete_page(&db, "pg-1").await.unwrap());
    }

    #[tokio::test]
    async fn list_is_scoped_and_ordered_by_position() {
        let db = db().await;
        // Insert out of order; listing must come back in ascending position.
        create_page(&db, &Page::new("c", "doc-1", "C", "", 3.0, "2026-01-01T00:00:03Z"))
            .await
            .unwrap();
        create_page(&db, &Page::new("a", "doc-1", "A", "", 1.0, "2026-01-01T00:00:01Z"))
            .await
            .unwrap();
        create_page(&db, &Page::new("b", "doc-1", "B", "", 2.0, "2026-01-01T00:00:02Z"))
            .await
            .unwrap();
        // A page on another document must not leak in.
        create_page(&db, &Page::new("z", "doc-2", "Z", "", 1.0, "2026-01-01T00:00:00Z"))
            .await
            .unwrap();

        let ids: Vec<String> = list_pages(&db, "doc-1")
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn reorder_is_a_single_position_update() {
        let db = db().await;
        create_page(&db, &Page::new("a", "doc-1", "A", "", 1.0, "2026-01-01T00:00:01Z"))
            .await
            .unwrap();
        create_page(&db, &Page::new("b", "doc-1", "B", "", 2.0, "2026-01-01T00:00:02Z"))
            .await
            .unwrap();
        create_page(&db, &Page::new("c", "doc-1", "C", "", 3.0, "2026-01-01T00:00:03Z"))
            .await
            .unwrap();

        // Move C to the front by writing a position before A — one row touched.
        let mut c = get_page(&db, "c").await.unwrap().unwrap();
        c.position = position_between(None, Some(1.0));
        c.updated_at = "2026-01-02T00:00:00Z".to_owned();
        update_page(&db, &c).await.unwrap();

        let ids: Vec<String> = list_pages(&db, "doc-1")
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, ["c", "a", "b"]);
    }

    #[tokio::test]
    async fn update_preserves_created_at_and_bumps_updated_at() {
        let db = db().await;
        let page = Page::new("pg-1", "doc-1", "Intro", "# Hello", 1.0, "2026-01-01T00:00:00Z");
        create_page(&db, &page).await.unwrap();

        let mut edited = page.clone();
        edited.body = "# Hello, world".to_owned();
        edited.updated_at = "2026-02-02T00:00:00Z".to_owned();
        let updated = update_page(&db, &edited).await.unwrap();
        assert_eq!(updated.body, "# Hello, world");
        // The creation stamp is immutable; the update stamp moves.
        assert_eq!(updated.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(updated.updated_at, "2026-02-02T00:00:00Z");

        let got = get_page(&db, "pg-1").await.unwrap().unwrap();
        assert_eq!(got, updated);
    }

    #[tokio::test]
    async fn update_unknown_is_not_found() {
        let db = db().await;
        let ghost = Page::new("ghost", "doc-1", "X", "", 1.0, "2026-01-01T00:00:00Z");
        let err = update_page(&db, &ghost).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::PageNotFound(_)));
    }

    #[test]
    fn fractional_positions_place_pages_correctly() {
        // Append after the last page, or onto an empty book.
        assert_eq!(append_position(None), 1.0);
        assert_eq!(append_position(Some(5.0)), 6.0);
        // Insert between, at the front, at the end, and into an empty book.
        assert_eq!(position_between(Some(1.0), Some(2.0)), 1.5);
        assert_eq!(position_between(None, Some(1.0)), 0.0);
        assert_eq!(position_between(Some(3.0), None), 4.0);
        assert_eq!(position_between(None, None), 1.0);
    }
}
