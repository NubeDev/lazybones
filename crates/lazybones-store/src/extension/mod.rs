//! Extensions: installed WASM backend (and optionally frontend) extension
//! metadata (design §3.5).
//!
//! Mirrors the [`asset`](crate::Asset) module shape — the `.wasm` component
//! bytes are a content-addressed [`BlobStore`](crate::BlobStore) blob keyed by
//! `wasm_sha256`, and this row is metadata only — with one deliberate difference:
//! installation is **strict by id** ([`create_extension`] errors on a duplicate),
//! because an extension id is an identity an admin grants capabilities against,
//! not a dedup key.
//!
//! The metadata is mirrored from the component's embedded `lazybones.ext.toml`
//! manifest on install, but the **embedded custom section stays authoritative**
//! for declared identity/caps; only [`set_extension_grants`] and
//! [`set_extension_enabled`] (admin decisions) mutate the row afterwards. The
//! typed capability vocabulary and the `granted ⊆ requested` enforcement live in
//! `lazybones-ext`; the store keeps capabilities as opaque wire strings.

mod create;
mod delete;
mod get;
mod list;
mod model;
mod row;
mod update;

pub use create::create_extension;
pub use delete::delete_extension;
pub use get::get_extension;
pub use list::list_extensions;
pub use model::{Extension, ExtensionSource, FrontendDescriptor};
pub use update::{set_extension_enabled, set_extension_grants};

#[cfg(test)]
mod tests {
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::error::StoreError;
    use crate::init_schema::init_schema;

    use super::*;

    async fn db() -> surrealdb::Surreal<surrealdb::engine::local::Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    fn sample() -> Extension {
        Extension::new(
            "ext-1",
            "Gate Guard",
            "0.1.0",
            "extension",
            vec!["gate-check".to_owned()],
            vec!["log".to_owned(), "store-read".to_owned()],
            "abc123",
            ExtensionSource::Url("https://example.com/ext.wasm".to_owned()),
            "2026-01-01T00:00:00Z",
        )
        .with_frontend(FrontendDescriptor {
            entry: "remoteEntry.js".to_owned(),
            exposed_module: "./mount".to_owned(),
            sdk_range: Some("^1.0".to_owned()),
            slots: vec!["task-detail.tab".to_owned()],
        })
    }

    #[tokio::test]
    async fn create_get_list_delete_roundtrip() {
        let db = db().await;
        let created = create_extension(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "ext-1");
        assert_eq!(created.wasm_sha256, "abc123");
        assert_eq!(created.exports, vec!["gate-check".to_owned()]);
        // Installs are disabled-by-default with no grants (default-deny §3.3).
        assert!(!created.enabled);
        assert!(created.granted_caps.is_empty());
        assert_eq!(
            created.source,
            ExtensionSource::Url("https://example.com/ext.wasm".to_owned())
        );

        let got = get_extension(&db, "ext-1").await.unwrap().unwrap();
        assert_eq!(got, created);
        assert_eq!(got.frontend.unwrap().exposed_module, "./mount");

        let all = list_extensions(&db, false).await.unwrap();
        assert_eq!(all.len(), 1);
        // Nothing enabled yet, so enabled-only is empty.
        assert!(list_extensions(&db, true).await.unwrap().is_empty());

        assert!(delete_extension(&db, "ext-1").await.unwrap());
        assert!(get_extension(&db, "ext-1").await.unwrap().is_none());
        assert!(!delete_extension(&db, "ext-1").await.unwrap());
    }

    #[tokio::test]
    async fn create_is_strict_on_duplicate_id() {
        let db = db().await;
        create_extension(&db, &sample()).await.unwrap();
        let err = create_extension(&db, &sample()).await.unwrap_err();
        assert!(matches!(err, StoreError::ExtensionExists(id) if id == "ext-1"));
    }

    #[tokio::test]
    async fn enable_and_grant_are_persisted() {
        let db = db().await;
        create_extension(&db, &sample()).await.unwrap();

        let enabled = set_extension_enabled(&db, "ext-1", true).await.unwrap();
        assert!(enabled.enabled);
        let only = list_extensions(&db, true).await.unwrap();
        assert_eq!(only.len(), 1);

        let granted = set_extension_grants(&db, "ext-1", vec!["log".to_owned()])
            .await
            .unwrap();
        assert_eq!(granted.granted_caps, vec!["log".to_owned()]);
        // The immutable manifest-mirrored fields survive the grant write.
        assert_eq!(granted.requested_caps.len(), 2);
        assert_eq!(granted.version, "0.1.0");
    }

    #[tokio::test]
    async fn mutating_a_missing_extension_is_not_found() {
        let db = db().await;
        let err = set_extension_enabled(&db, "ghost", true).await.unwrap_err();
        assert!(matches!(err, StoreError::ExtensionNotFound(id) if id == "ghost"));
        let err = set_extension_grants(&db, "ghost", vec![]).await.unwrap_err();
        assert!(matches!(err, StoreError::ExtensionNotFound(id) if id == "ghost"));
    }

    #[tokio::test]
    async fn backend_only_extension_has_no_frontend() {
        let db = db().await;
        let ext = Extension::new(
            "ext-2",
            "Headless",
            "1.2.3",
            "extension",
            vec!["event-reaction".to_owned()],
            vec!["log".to_owned()],
            "deadbeef",
            ExtensionSource::Upload,
            "2026-02-02T00:00:00Z",
        );
        let created = create_extension(&db, &ext).await.unwrap();
        assert!(created.frontend.is_none());
        let got = get_extension(&db, "ext-2").await.unwrap().unwrap();
        assert_eq!(got, created);
        assert_eq!(got.source, ExtensionSource::Upload);
    }
}
