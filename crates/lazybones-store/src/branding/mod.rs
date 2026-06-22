//! Branding: standalone, global brand profiles (logo + colors + fonts +
//! header/footer) and the verbs that create/read/list/update/delete them.
//!
//! Branding is **cross-cutting, not a doc-writer subfeature**: the user maintains
//! many brand profiles as an app-wide resource and any feature references one by
//! id. Mirrors the [`skill`](crate::skill) CRUD shape; `seed.rs` seeds one neutral
//! default brand so there is always one to pick.

mod create;
mod delete;
mod get;
mod list;
mod model;
mod row;
mod seed;
mod update;

pub use create::create_branding;
pub use delete::delete_branding;
pub use get::get_branding;
pub use list::list_branding;
pub use model::{BrandColors, BrandFonts, Branding};
pub use seed::seed_default_branding;
pub use update::update_branding;

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

    fn sample() -> Branding {
        Branding::new("acme", "Acme Corp", "2026-01-01T00:00:00Z").with_colors(BrandColors {
            primary: "#ff0000".into(),
            ..BrandColors::default()
        })
    }

    #[tokio::test]
    async fn create_get_list_delete_roundtrip() {
        let db = db().await;
        let created = create_branding(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "acme");
        assert_eq!(created.colors.primary, "#ff0000");
        assert_eq!(created.project, None);

        let got = get_branding(&db, "acme").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_branding(&db, None).await.unwrap();
        assert_eq!(all.len(), 1);

        assert!(delete_branding(&db, "acme").await.unwrap());
        assert!(get_branding(&db, "acme").await.unwrap().is_none());
        // Deleting again reports "did not exist".
        assert!(!delete_branding(&db, "acme").await.unwrap());
    }

    #[tokio::test]
    async fn colors_and_fonts_roundtrip_as_json_columns() {
        let db = db().await;
        let brand = Branding::new("brand-x", "Brand X", "2026-01-01T00:00:00Z")
            .with_colors(BrandColors {
                primary: "#123456".into(),
                accent: "#abcdef".into(),
                ..BrandColors::default()
            })
            .with_fonts(BrandFonts { heading: "Georgia".into(), body: "Arial".into() });
        create_branding(&db, &brand).await.unwrap();
        let got = get_branding(&db, "brand-x").await.unwrap().unwrap();
        assert_eq!(got.colors.primary, "#123456");
        assert_eq!(got.colors.accent, "#abcdef");
        assert_eq!(got.fonts.heading, "Georgia");
        assert_eq!(got.fonts.body, "Arial");
    }

    #[tokio::test]
    async fn create_duplicate_is_error() {
        let db = db().await;
        create_branding(&db, &sample()).await.unwrap();
        let err = create_branding(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::BrandingExists(_)));
    }

    #[tokio::test]
    async fn update_edits_fields_and_preserves_created_at() {
        let db = db().await;
        create_branding(&db, &sample()).await.unwrap();

        let mut edited = Branding::new("acme", "Acme (revised)", "2026-02-02T00:00:00Z");
        edited.header_text = "Confidential".into();
        let updated = update_branding(&db, &edited).await.unwrap();
        assert_eq!(updated.name, "Acme (revised)");
        assert_eq!(updated.header_text, "Confidential");
        // The creation stamp is immutable; only `updated_at` moves.
        assert_eq!(updated.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(updated.updated_at, "2026-02-02T00:00:00Z");

        let got = get_branding(&db, "acme").await.unwrap().unwrap();
        assert_eq!(got, updated);
    }

    #[tokio::test]
    async fn update_unknown_is_not_found() {
        let db = db().await;
        let err = update_branding(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, crate::StoreError::BrandingNotFound(_)));
    }

    #[tokio::test]
    async fn seed_creates_default_then_is_idempotent_and_non_clobbering() {
        let db = db().await;
        // First seed creates the neutral default brand.
        let n = seed_default_branding(&db, "2026-01-01T00:00:00Z").await.unwrap();
        assert_eq!(n, 1);
        let seeded = list_branding(&db, None).await.unwrap();
        assert_eq!(seeded.len(), 1);
        assert_eq!(seeded[0].id, "default");

        // An operator edits the seeded brand.
        let mut edited = get_branding(&db, "default").await.unwrap().unwrap();
        edited.name = "MINE".into();
        update_branding(&db, &edited).await.unwrap();

        // Re-seeding creates nothing new and leaves the edit intact.
        let n2 = seed_default_branding(&db, "2026-04-04T00:00:00Z").await.unwrap();
        assert_eq!(n2, 0, "re-seed must not recreate existing ids");
        let after = get_branding(&db, "default").await.unwrap().unwrap();
        assert_eq!(after.name, "MINE", "operator edit must survive re-seed");
    }
}
