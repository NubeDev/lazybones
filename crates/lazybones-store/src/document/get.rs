//! Read a single document by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Document;
use super::row::{DOCUMENT_TABLE, DocumentRow};

/// Read `document:<id>`, or `None` if no such document exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_document(db: &Surreal<Db>, id: &str) -> Result<Option<Document>> {
    let row: Option<DocumentRow> = db
        .select((DOCUMENT_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(DocumentRow::into_document))
}
