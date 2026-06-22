//! Creation of a source row (a document's upload / context item).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Source;
use super::row::{SOURCE_TABLE, SourceRow};

/// Create the row for `source`. Source ids are caller-minted (e.g. a ULID) and
/// many sources hang off one document, so this is a plain insert — there is no
/// install-wide "already exists" semantics to guard (unlike a skill/document id).
///
/// Returns the stored source as it is after the write.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails, or
/// [`StoreError::SourceNotFound`] if the insert unexpectedly returns no row.
pub async fn create_source(db: &Surreal<Db>, source: &Source) -> Result<Source> {
    let written: Option<SourceRow> = db
        .create((SOURCE_TABLE, source.id.as_str()))
        .content(SourceRow::from_source(source))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(SourceRow::into_source)
        .ok_or_else(|| StoreError::SourceNotFound(source.id.clone()))
}
