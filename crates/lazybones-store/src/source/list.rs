//! List a document's sources (`GET /documents/:id/sources`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Source;
use super::row::{SOURCE_TABLE, SourceRow};

/// List the sources behind `document`, newest first.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_sources(db: &Surreal<Db>, document: &str) -> Result<Vec<Source>> {
    let rows: Vec<SourceRow> = db
        .query(format!(
            "SELECT * FROM {SOURCE_TABLE} WHERE document = $document ORDER BY created_at DESC"
        ))
        .bind(("document", document.to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(SourceRow::into_source).collect())
}
