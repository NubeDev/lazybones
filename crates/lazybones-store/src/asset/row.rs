//! The persisted shape of an [`Asset`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Asset`] carries
//! a plain string id. `size` is stored as `i64` (SurrealDB's integer type);
//! `Option` columns keep the row forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::Asset;

/// The table asset metadata lives in.
pub(crate) const ASSET_TABLE: &str = "asset";

/// SurrealDB-facing asset: the reserved `id` thing plus the file metadata.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct AssetRow {
    pub(crate) id: RecordId,
    pub(crate) project: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) content_type: Option<String>,
    /// Byte length, stored as `int` (`i64`); file sizes fit comfortably.
    pub(crate) size: Option<i64>,
    pub(crate) sha256: Option<String>,
    pub(crate) created_at: Option<String>,
}

impl AssetRow {
    /// Project a domain [`Asset`] into its persisted row.
    pub(crate) fn from_asset(a: &Asset) -> Self {
        Self {
            id: RecordId::new(ASSET_TABLE, a.id.as_str()),
            project: a.project.clone(),
            filename: Some(a.filename.clone()),
            content_type: Some(a.content_type.clone()),
            size: i64::try_from(a.size).ok(),
            sha256: Some(a.sha256.clone()),
            created_at: Some(a.created_at.clone()),
        }
    }

    /// Reconstruct the domain [`Asset`].
    pub(crate) fn into_asset(self) -> Asset {
        Asset {
            id: asset_key(&self.id),
            project: self.project,
            filename: self.filename.unwrap_or_default(),
            content_type: self.content_type.unwrap_or_default(),
            size: self.size.and_then(|s| u64::try_from(s).ok()).unwrap_or(0),
            sha256: self.sha256.unwrap_or_default(),
            created_at: self.created_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of an asset id's key (the part after `asset:`).
fn asset_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
