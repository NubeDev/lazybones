//! The single-record user preferences: operator UI choices (timezone, theme)
//! that follow them across browsers, plus the get/put verbs. Shares the
//! `settings` table with the management-agent config, keyed by a constant.

mod get;
mod model;
mod put;
mod row;

pub use get::get_preferences;
pub use model::Preferences;
pub use put::put_preferences;

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
        assert!(get_preferences(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn put_then_get_roundtrip_and_overwrite() {
        let db = db().await;
        let prefs = Preferences {
            timezone: Some("Asia/Ho_Chi_Minh".into()),
            theme: Some("dark".into()),
            updated_at: "2026-06-21T00:00:00Z".into(),
        };
        let written = put_preferences(&db, &prefs).await.unwrap();
        assert_eq!(written, prefs);

        let got = get_preferences(&db).await.unwrap().unwrap();
        assert_eq!(got, prefs);

        // Overwriting replaces the single record in place.
        let prefs2 = Preferences {
            timezone: None,
            ..prefs
        };
        put_preferences(&db, &prefs2).await.unwrap();
        let got2 = get_preferences(&db).await.unwrap().unwrap();
        assert!(got2.timezone.is_none());
        assert_eq!(got2.theme.as_deref(), Some("dark"));
    }
}
