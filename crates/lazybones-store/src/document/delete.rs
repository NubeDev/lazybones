//! Delete a document by id (`DELETE /documents/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{DOCUMENT_TABLE, DocumentRow};

/// Delete `document:<id>`. Returns whether a document existed.
///
/// Note: this does **not** cascade to the [`attachment`](crate::attachment) rows
/// (attached references) or the [`source`](crate::source) rows behind it — those
/// are polymorphic and carry no hard FK, so a dangling link is tolerated.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_document(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<DocumentRow> = db
        .delete((DOCUMENT_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
