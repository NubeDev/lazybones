//! Delete a source by id (`DELETE /documents/:id/sources/:sid`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{SOURCE_TABLE, SourceRow};

/// Delete `source:<id>`. Returns whether a source existed.
///
/// This removes the metadata row only; the backing [`Asset`](crate::Asset) (for a
/// file source) and its blob bytes are content-addressed and may still be shared,
/// so removing them is the caller's separate decision.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_source(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<SourceRow> = db
        .delete((SOURCE_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
