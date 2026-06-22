//! Orgs: the root containment node of the team graph, its persisted row, and the
//! thin create/get/list verbs that author and read them. An org is a cloud-only
//! graph node (single writer) — it keeps a plain id (D4 namespacing applies only
//! to syncable, edge-minted rows). Containment (`team ->under-> org`) lives on the
//! [`team`](crate::team) module's `under` helpers.

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
        let created = create_org(&db, &Org::new("acme", "Acme Inc", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(created.id, "acme");
        assert_eq!(created.name, "Acme Inc");

        let got = get_org(&db, "acme").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_orgs(&db).await.unwrap();
        assert_eq!(all.len(), 1);
        assert!(get_org(&db, "missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn create_duplicate_is_error() {
        let db = db().await;
        let org = Org::new("acme", "Acme Inc", "2026-01-01T00:00:00Z");
        create_org(&db, &org).await.unwrap();
        let err = create_org(&db, &org).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::OrgExists(_)));
    }
}
