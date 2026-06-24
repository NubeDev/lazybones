//! Delete a page by id (`DELETE /documents/:id/pages/:pid`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{PAGE_TABLE, PageRow};

/// Delete `page:<id>`. Returns whether a page existed.
///
/// Removing a page leaves a gap in the fractional `position` sequence, which is
/// harmless — the remaining pages keep their relative order.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_page(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<PageRow> = db
        .delete((PAGE_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
