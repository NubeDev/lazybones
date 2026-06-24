//! Read a single extension's metadata by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Extension;
use super::row::{EXTENSION_TABLE, ExtensionRow};

/// Read `extension:<id>`, or `None` if no such extension exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_extension(db: &Surreal<Db>, id: &str) -> Result<Option<Extension>> {
    let row: Option<ExtensionRow> = db
        .select((EXTENSION_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(ExtensionRow::into_extension))
}
