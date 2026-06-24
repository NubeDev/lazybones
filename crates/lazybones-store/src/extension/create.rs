//! Strict installation of an extension metadata row (must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Extension;
use super::row::{EXTENSION_TABLE, ExtensionRow};

/// Create `extension` as a new record, failing if its id is already in use.
///
/// This persists **metadata only**; the `.wasm` component bytes are written
/// separately via [`BlobStore::put`](crate::BlobStore::put), content-addressed by
/// `extension.wasm_sha256` (design §3.5). The API layer orchestrates both.
///
/// Returns the stored extension as it is after the write.
///
/// # Errors
/// Returns [`StoreError::ExtensionExists`] if an extension with that id already
/// exists, or [`StoreError::Operation`] if the read or write fails.
pub async fn create_extension(db: &Surreal<Db>, extension: &Extension) -> Result<Extension> {
    let existing: Option<ExtensionRow> = db
        .select((EXTENSION_TABLE, extension.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::ExtensionExists(extension.id.clone()));
    }

    let written: Option<ExtensionRow> = db
        .create((EXTENSION_TABLE, extension.id.as_str()))
        .content(ExtensionRow::from_extension(extension))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(ExtensionRow::into_extension)
        .ok_or_else(|| StoreError::ExtensionNotFound(extension.id.clone()))
}
