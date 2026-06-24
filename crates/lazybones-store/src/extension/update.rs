//! Mutate the two admin-controlled fields of an installed extension: its
//! `enabled` flag (`/enable` · `/disable`) and its `granted_caps`
//! (`POST /extensions/:id/grants`).
//!
//! Everything else on the row is mirrored from the embedded manifest and is
//! immutable here — the embedded (and, once signing exists, signed) section is the
//! source of truth for declared identity/caps (design §3.5). Re-installing is how
//! that metadata changes.
//!
//! NOTE: `granted_caps ⊆ requested_caps` is enforced by `lazybones-ext` at grant
//! time (it owns the typed capability vocabulary); this verb persists the grant
//! the caller has already validated.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Extension;
use super::row::{EXTENSION_TABLE, ExtensionRow};

/// Flip `extension:<id>`'s `enabled` flag, failing if no such extension exists.
///
/// Returns the stored extension as it is after the write.
///
/// # Errors
/// Returns [`StoreError::ExtensionNotFound`] if no extension with that id exists,
/// or [`StoreError::Operation`] if the read or write fails.
pub async fn set_extension_enabled(
    db: &Surreal<Db>,
    id: &str,
    enabled: bool,
) -> Result<Extension> {
    let mut row = load(db, id).await?;
    row.enabled = Some(enabled);
    write(db, id, row).await
}

/// Replace `extension:<id>`'s `granted_caps`, failing if no such extension
/// exists. The caller must have already validated `granted ⊆ requested_caps`
/// (see [`lazybones-ext`]).
///
/// Returns the stored extension as it is after the write.
///
/// # Errors
/// Returns [`StoreError::ExtensionNotFound`] if no extension with that id exists,
/// or [`StoreError::Operation`] if the read or write fails.
pub async fn set_extension_grants(
    db: &Surreal<Db>,
    id: &str,
    granted_caps: Vec<String>,
) -> Result<Extension> {
    let mut row = load(db, id).await?;
    row.granted_caps = Some(granted_caps);
    write(db, id, row).await
}

/// Load the current row or map a missing id to [`StoreError::ExtensionNotFound`].
async fn load(db: &Surreal<Db>, id: &str) -> Result<ExtensionRow> {
    let existing: Option<ExtensionRow> = db
        .select((EXTENSION_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    existing.ok_or_else(|| StoreError::ExtensionNotFound(id.to_owned()))
}

/// Persist `row` back to `extension:<id>` and return the reconstructed domain
/// value.
async fn write(db: &Surreal<Db>, id: &str, row: ExtensionRow) -> Result<Extension> {
    let written: Option<ExtensionRow> = db
        .update((EXTENSION_TABLE, id.to_owned()))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;
    written
        .map(ExtensionRow::into_extension)
        .ok_or_else(|| StoreError::ExtensionNotFound(id.to_owned()))
}
