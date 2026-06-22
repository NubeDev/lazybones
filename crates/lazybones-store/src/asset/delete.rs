//! Delete an asset's metadata by id (`DELETE /assets/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{ASSET_TABLE, AssetRow};

/// Delete `asset:<id>` metadata. Returns whether an asset existed.
///
/// This removes the **metadata row only**; the underlying blob bytes are deleted
/// separately via [`BlobStore::delete`](super::BlobStore::delete) (the API
/// orchestrates both — the content-addressed blob may still be shared by another
/// project scope, so blob deletion is the caller's decision).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_asset(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<AssetRow> = db
        .delete((ASSET_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
