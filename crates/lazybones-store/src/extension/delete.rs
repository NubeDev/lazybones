//! Delete an extension's metadata by id (`DELETE /extensions/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{EXTENSION_TABLE, ExtensionRow};

/// Delete `extension:<id>` metadata. Returns whether an extension existed.
///
/// This removes the **metadata row only**; the underlying `.wasm` (and frontend)
/// blob bytes are deleted separately via
/// [`BlobStore::delete`](crate::BlobStore::delete). Because the blob is
/// content-addressed it may still be shared, so blob deletion is the caller's
/// decision (mirrors [`delete_asset`](crate::Asset)).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_extension(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<ExtensionRow> = db
        .delete((EXTENSION_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
