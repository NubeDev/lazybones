//! Strict creation of a document (must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Document;
use super::row::{DOCUMENT_TABLE, DocumentRow};

/// Create `document` as a new record, failing if its id is already in use.
///
/// Returns the stored document as it is after the write.
///
/// # Errors
/// Returns [`StoreError::DocumentExists`] if a document with that id already
/// exists, or [`StoreError::Operation`] if the read or write fails.
pub async fn create_document(db: &Surreal<Db>, document: &Document) -> Result<Document> {
    let existing: Option<DocumentRow> = db
        .select((DOCUMENT_TABLE, document.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::DocumentExists(document.id.clone()));
    }

    let written: Option<DocumentRow> = db
        .create((DOCUMENT_TABLE, document.id.as_str()))
        .content(DocumentRow::from_document(document))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(DocumentRow::into_document)
        .ok_or_else(|| StoreError::DocumentNotFound(document.id.clone()))
}
