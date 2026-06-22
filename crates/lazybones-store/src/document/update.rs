//! Edit an existing document, preserving `created_at`.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Document;
use super::row::{DOCUMENT_TABLE, DocumentRow};

/// Overwrite the editable fields of `document:<id>`, failing if no such document
/// exists. The original `created_at` is preserved; `document.updated_at` is
/// stored as the new update stamp. The whole [`DocRepo`](super::DocRepo) (target
/// + the `branch`/`*_url` linkage filled in by GitHub actions) round-trips here.
///
/// Returns the stored document as it is after the write.
///
/// # Errors
/// Returns [`StoreError::DocumentNotFound`] if no document with that id exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_document(db: &Surreal<Db>, document: &Document) -> Result<Document> {
    let existing: Option<DocumentRow> = db
        .select((DOCUMENT_TABLE, document.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    let Some(existing) = existing else {
        return Err(StoreError::DocumentNotFound(document.id.clone()));
    };

    // Preserve the immutable creation stamp regardless of what the caller sent.
    let mut row = DocumentRow::from_document(document);
    row.created_at = existing.created_at;

    let written: Option<DocumentRow> = db
        .update((DOCUMENT_TABLE, document.id.as_str()))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(DocumentRow::into_document)
        .ok_or_else(|| StoreError::DocumentNotFound(document.id.clone()))
}
