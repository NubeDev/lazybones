//! Users: a person in the team graph, their persisted row, and the thin
//! create/get/list verbs membership needs. Mirrors the [`skill`](crate::skill)
//! module shape. Team membership (`user ->member_of-> team`) lives on the
//! [`team`](crate::team) `member_of` helper, not here.

mod create;
mod get;
mod list;
mod model;
mod row;

pub use create::create_user;
pub use get::get_user;
pub use list::list_users;
pub use model::User;

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
        let created = create_user(&db, &User::new("ada", "Ada Lovelace", "2026-01-01T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(created.id, "ada");
        assert!(!created.admin);

        let got = get_user(&db, "ada").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_users(&db).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn admin_flag_roundtrips() {
        let db = db().await;
        let admin = User::new("root", "Root", "2026-01-01T00:00:00Z").as_admin();
        create_user(&db, &admin).await.unwrap();
        let got = get_user(&db, "root").await.unwrap().unwrap();
        assert!(got.admin);
    }
}
