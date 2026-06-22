//! The durable `Asset` document — content-addressed file metadata.
//!
//! An asset is a single uploaded file (a logo, a diagram) stored once and reused.
//! The **bytes** live behind the [`BlobStore`](super::BlobStore), content-addressed
//! by `sha256`; this row is **metadata only**. Creation is content-addressed
//! (see [`create_asset`](super::create_asset)): two uploads of identical bytes
//! dedup to one asset — that is what makes "reusable images" free. There is no
//! `updated_at`: an asset's content is immutable (a new file is a new asset);
//! only its descriptive metadata can be edited in place.

use serde::{Deserialize, Serialize};

/// File metadata for a content-addressed asset, unique install-wide by `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Friendly, unique id (typically minted by the caller, e.g. a ULID).
    pub id: String,
    /// Optional project scope; always `None` today (projects land later). Part of
    /// the dedup key, so identical bytes in different projects are distinct rows.
    #[serde(default)]
    pub project: Option<String>,
    /// The original upload filename (descriptive; not the storage key).
    pub filename: String,
    /// The MIME content type, e.g. `image/png`.
    pub content_type: String,
    /// The byte length of the stored blob.
    pub size: u64,
    /// The hex SHA-256 of the bytes — the content address and dedup key.
    pub sha256: String,
    /// RFC3339 creation timestamp.
    pub created_at: String,
}

impl Asset {
    /// A freshly uploaded asset stamped `created_at == now`.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
        size: u64,
        sha256: impl Into<String>,
        now: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            project: None,
            filename: filename.into(),
            content_type: content_type.into(),
            size,
            sha256: sha256.into(),
            created_at: now.into(),
        }
    }
}
