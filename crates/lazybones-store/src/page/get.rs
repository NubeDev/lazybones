//! Read a single page by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Page;
use super::row::{PAGE_TABLE, PageRow};

/// Read `page:<id>`, or `None` if no such page exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_page(db: &Surreal<Db>, id: &str) -> Result<Option<Page>> {
    let row: Option<PageRow> = db
        .select((PAGE_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(PageRow::into_page))
}
