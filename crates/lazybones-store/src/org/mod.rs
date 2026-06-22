//! Orgs: the root container of the team graph, its persisted row, and the thin
//! create/get/list verbs containment needs. Mirrors the [`skill`](crate::skill)
//! module shape (model/row/verb-per-file). Containment edges (`team ->under-> org`)
//! live on the [`team`](crate::team) `under` helper, not here.

mod create;
mod get;
mod list;
mod model;
mod row;

pub use create::create_org;
pub use get::get_org;
pub use list::list_orgs;
pub use model::Org;

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
    async fn create_get_list_roundtrip() {
        let db = db().await;
        let created = create_org(&db, &Org::new("nube", "Nube", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(created.id, "nube");
        assert_eq!(created.title, "Nube");

        let got = get_org(&db, "nube").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_orgs(&db).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn create_is_idempotent() {
        let db = db().await;
        create_org(&db, &Org::new("nube", "Nube", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        // Re-creating the same id returns the stored record rather than erroring
        // (cloud-authored, single-writer org graph, decisions §3).
        let again = create_org(&db, &Org::new("nube", "Ignored", "2026-02-02T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(again.title, "Nube");
        assert_eq!(list_orgs(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_unknown_is_none() {
        let db = db().await;
        assert!(get_org(&db, "ghost").await.unwrap().is_none());
    }
}
