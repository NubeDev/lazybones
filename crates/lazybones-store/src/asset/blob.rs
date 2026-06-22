//! Content-addressed blob storage for asset bytes — kept **outside** the
//! relational rows so the embedded KV store isn't bloated by binary payloads.
//!
//! The [`Asset`](super::Asset) metadata row lives in SurrealDB; the bytes live
//! behind the [`BlobStore`] trait, content-addressed by sha256 (this is what
//! makes "reusable images" dedup for free). The default [`FileBlobStore`] writes
//! under `{root}/assets/{project-prefix}/{sha256}`; the trait can be swapped for
//! S3 or a SurrealDB `DEFINE BUCKET` backend later without touching the asset
//! metadata, routes, or UI. Project becomes a key prefix.
//!
//! The trait is `async` and returns `Result<_, AssetError>`, mirroring how the
//! store verbs return `Result<_, StoreError>`. The API layer maps `AssetError`
//! onto HTTP status codes (404 for not-found, 500 for IO).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sha2::{Digest, Sha256};

/// Failures raised by a [`BlobStore`] backend.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssetError {
    /// No blob exists for the requested sha256 (+project).
    #[error("blob not found: {0}")]
    NotFound(String),

    /// An underlying IO operation failed.
    #[error("blob io failed: {0}")]
    Io(#[from] std::io::Error),
}

/// The hex-encoded SHA-256 of `bytes` — the content address an [`Asset`] is keyed
/// by, computed once on upload and stored on the metadata row.
///
/// [`Asset`]: super::Asset
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A swappable, content-addressed byte store for asset payloads.
///
/// Implementations are keyed by `sha256` and scoped by an optional `project`
/// (which becomes a storage-key prefix). The trait is object-safe (used behind
/// `Arc<dyn BlobStore>` in the API), hence [`async_trait`].
#[async_trait]
pub trait BlobStore: Send + Sync {
    /// Write `bytes` under `sha256` (+`project`). Idempotent: re-putting the same
    /// content is a harmless overwrite of identical bytes.
    ///
    /// # Errors
    /// Returns [`AssetError::Io`] if the write fails.
    async fn put(&self, sha256: &str, project: Option<&str>, bytes: &[u8])
    -> Result<(), AssetError>;

    /// Read the bytes stored under `sha256` (+`project`).
    ///
    /// # Errors
    /// Returns [`AssetError::NotFound`] if no such blob exists, or
    /// [`AssetError::Io`] on a read failure.
    async fn get(&self, sha256: &str, project: Option<&str>) -> Result<Vec<u8>, AssetError>;

    /// Delete the blob stored under `sha256` (+`project`). Returns whether one
    /// existed.
    ///
    /// # Errors
    /// Returns [`AssetError::Io`] if the delete fails for a reason other than the
    /// blob being absent.
    async fn delete(&self, sha256: &str, project: Option<&str>) -> Result<bool, AssetError>;
}

/// The default file-backed [`BlobStore`]: bytes under
/// `{root}/assets/{project-prefix}/{sha256}`.
#[derive(Debug, Clone)]
pub struct FileBlobStore {
    root: PathBuf,
}

impl FileBlobStore {
    /// A file blob store rooted at `root` (typically the daemon's `data_dir`).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The on-disk path a blob lives at. `None` project maps to a `_global`
    /// prefix so the layout is uniform once projects land.
    fn path_for(&self, sha256: &str, project: Option<&str>) -> PathBuf {
        self.root
            .join("assets")
            .join(project.unwrap_or("_global"))
            .join(sha256)
    }
}

#[async_trait]
impl BlobStore for FileBlobStore {
    async fn put(
        &self,
        sha256: &str,
        project: Option<&str>,
        bytes: &[u8],
    ) -> Result<(), AssetError> {
        let path = self.path_for(sha256, project);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes)?;
        Ok(())
    }

    async fn get(&self, sha256: &str, project: Option<&str>) -> Result<Vec<u8>, AssetError> {
        let path = self.path_for(sha256, project);
        read_or_not_found(&path, sha256)
    }

    async fn delete(&self, sha256: &str, project: Option<&str>) -> Result<bool, AssetError> {
        let path = self.path_for(sha256, project);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(AssetError::Io(e)),
        }
    }
}

/// Read `path`, mapping a missing file to [`AssetError::NotFound`].
fn read_or_not_found(path: &Path, sha256: &str) -> Result<Vec<u8>, AssetError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(AssetError::NotFound(sha256.to_owned()))
        }
        Err(e) => Err(AssetError::Io(e)),
    }
}
