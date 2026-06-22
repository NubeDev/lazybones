//! List documents (`GET /documents`), optionally narrowed by project.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Document;
use super::row::{DOCUMENT_TABLE, DocumentRow};

/// List every document, optionally narrowed to one `project` scope. Passing
/// `None` lists across all scopes (the only behaviour until projects land).
/// References and normal documents are both returned; callers filter by
/// [`kind`](super::DocKind) as needed.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_documents(db: &Surreal<Db>, project: Option<&str>) -> Result<Vec<Document>> {
    let mut sql = format!("SELECT * FROM {DOCUMENT_TABLE}");
    if project.is_some() {
        sql.push_str(" WHERE project = $project");
    }
    let rows: Vec<DocumentRow> = db
        .query(sql)
        .bind(("project", project.map(ToOwned::to_owned)))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(DocumentRow::into_document).collect())
}
