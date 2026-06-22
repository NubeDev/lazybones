//! Assets: content-addressed file metadata (a logo/image stored once and reused)
//! plus the [`BlobStore`] seam that holds the bytes outside the relational rows.
//!
//! Mirrors the [`skill`](crate::skill) module shape, with two deliberate
//! differences: creation is **content-addressed** ([`create_asset`] dedups on
//! sha256 rather than erroring on a duplicate), and there is **no update** — an
//! asset's bytes are immutable, so a changed file is simply a new asset. The
//! bytes themselves live behind [`BlobStore`] (see [`blob`]).

mod blob;
mod create;
mod delete;
mod get;
mod list;
mod model;
mod row;

pub use blob::{AssetError, BlobStore, FileBlobStore, sha256_hex};
pub use create::create_asset;
pub use delete::delete_asset;
pub use get::get_asset;
pub use list::list_assets;
pub use model::Asset;

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

    fn sample() -> Asset {
        Asset::new(
            "asset-1",
            "logo.png",
            "image/png",
            1234,
            "abc123",
            "2026-01-01T00:00:00Z",
        )
    }

    #[tokio::test]
    async fn create_get_list_delete_roundtrip() {
        let db = db().await;
        let created = create_asset(&db, &sample()).await.unwrap();
        assert_eq!(created.id, "asset-1");
        assert_eq!(created.sha256, "abc123");
        assert_eq!(created.size, 1234);
        assert_eq!(created.project, None);

        let got = get_asset(&db, "asset-1").await.unwrap().unwrap();
        assert_eq!(got, created);

        let all = list_assets(&db, None).await.unwrap();
        assert_eq!(all.len(), 1);

        assert!(delete_asset(&db, "asset-1").await.unwrap());
        assert!(get_asset(&db, "asset-1").await.unwrap().is_none());
        // Deleting again reports "did not exist".
        assert!(!delete_asset(&db, "asset-1").await.unwrap());
    }

    #[tokio::test]
    async fn create_is_content_addressed_and_dedups() {
        let db = db().await;
        let first = create_asset(&db, &sample()).await.unwrap();

        // A second upload of identical bytes (same sha256), even with a different
        // id/filename, returns the *existing* asset — no duplicate row.
        let dup = Asset::new(
            "asset-2",
            "logo-copy.png",
            "image/png",
            1234,
            "abc123",
            "2026-02-02T00:00:00Z",
        );
        let deduped = create_asset(&db, &dup).await.unwrap();
        assert_eq!(deduped.id, first.id, "dedup returns the first asset");

        let all = list_assets(&db, None).await.unwrap();
        assert_eq!(all.len(), 1, "one row, not two");
    }

    #[tokio::test]
    async fn file_blob_store_put_get_delete_roundtrip() {
        let root = std::env::temp_dir().join(format!("lazybones-blob-{}", sha256_hex(b"asset-test")));
        let store = FileBlobStore::new(&root);
        let bytes = b"hello blob world";
        let sha = sha256_hex(bytes);

        store.put(&sha, None, bytes).await.unwrap();
        let back = store.get(&sha, None).await.unwrap();
        assert_eq!(back, bytes);

        assert!(store.delete(&sha, None).await.unwrap());
        // Reading a missing blob is a typed not-found, not an IO panic.
        let err = store.get(&sha, None).await.unwrap_err();
        assert!(matches!(err, AssetError::NotFound(_)));
        // Deleting again reports "did not exist".
        assert!(!store.delete(&sha, None).await.unwrap());

        let _ = std::fs::remove_dir_all(&root);
    }
}
