//! Edit an existing page, preserving `created_at`.
//!
//! This carries the full [`Page`] — editing the `body`/`title` *or* moving it by
//! writing a new fractional [`position`](super::Page::position) (see
//! [`position_between`](super::position_between)) both go through here, so a
//! reorder is a one-row write.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Page;
use super::row::{PAGE_TABLE, PageRow};

/// Overwrite the editable fields of `page:<id>`, failing if no such page exists.
/// The original `created_at` is preserved; `page.updated_at` is stored as the new
/// update stamp.
///
/// Returns the stored page as it is after the write.
///
/// # Errors
/// Returns [`StoreError::PageNotFound`] if no page with that id exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_page(db: &Surreal<Db>, page: &Page) -> Result<Page> {
    let existing: Option<PageRow> = db
        .select((PAGE_TABLE, page.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    let Some(existing) = existing else {
        return Err(StoreError::PageNotFound(page.id.clone()));
    };

    // Preserve the immutable creation stamp regardless of what the caller sent.
    let mut row = PageRow::from_page(page);
    row.created_at = existing.created_at;

    let written: Option<PageRow> = db
        .update((PAGE_TABLE, page.id.as_str()))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(PageRow::into_page)
        .ok_or_else(|| StoreError::PageNotFound(page.id.clone()))
}
