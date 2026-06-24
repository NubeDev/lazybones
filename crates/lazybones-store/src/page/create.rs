//! Creation of a page row (one ordered section of a document).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Page;
use super::row::{PAGE_TABLE, PageRow};

/// Create the row for `page`. Page ids are caller-minted (e.g. a ULID) and many
/// pages hang off one document, so this is a plain insert — there is no
/// install-wide "already exists" semantics to guard (unlike a document id).
///
/// Returns the stored page as it is after the write.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails, or
/// [`StoreError::PageNotFound`] if the insert unexpectedly returns no row.
pub async fn create_page(db: &Surreal<Db>, page: &Page) -> Result<Page> {
    let written: Option<PageRow> = db
        .create((PAGE_TABLE, page.id.as_str()))
        .content(PageRow::from_page(page))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(PageRow::into_page)
        .ok_or_else(|| StoreError::PageNotFound(page.id.clone()))
}
