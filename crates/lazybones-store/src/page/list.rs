//! List a document's pages in render order (`GET /documents/:id/pages`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Page;
use super::row::{PAGE_TABLE, PageRow};

/// List the pages of `document` in ascending `position` order — i.e. the order
/// they render in the exported book. Ties (equal positions) break by `created_at`
/// so the order is always deterministic.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_pages(db: &Surreal<Db>, document: &str) -> Result<Vec<Page>> {
    let rows: Vec<PageRow> = db
        .query(format!(
            "SELECT * FROM {PAGE_TABLE} WHERE document = $document \
             ORDER BY position ASC, created_at ASC"
        ))
        .bind(("document", document.to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(PageRow::into_page).collect())
}
