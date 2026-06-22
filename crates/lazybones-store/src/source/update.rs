//! Edit an existing source, preserving `created_at`.
//!
//! The common edit is back-filling [`extracted_text`](super::Source) after an
//! out-of-band PDF extraction, or relabelling a source's `title`.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Source;
use super::row::{SOURCE_TABLE, SourceRow};

/// Overwrite the editable fields of `source:<id>`, failing if no such source
/// exists. The original `created_at` is preserved.
///
/// Returns the stored source as it is after the write.
///
/// # Errors
/// Returns [`StoreError::SourceNotFound`] if no source with that id exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_source(db: &Surreal<Db>, source: &Source) -> Result<Source> {
    let existing: Option<SourceRow> = db
        .select((SOURCE_TABLE, source.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    let Some(existing) = existing else {
        return Err(StoreError::SourceNotFound(source.id.clone()));
    };

    // Preserve the immutable creation stamp regardless of what the caller sent.
    let mut row = SourceRow::from_source(source);
    row.created_at = existing.created_at;

    let written: Option<SourceRow> = db
        .update((SOURCE_TABLE, source.id.as_str()))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(SourceRow::into_source)
        .ok_or_else(|| StoreError::SourceNotFound(source.id.clone()))
}
