//! Delete a template by id (`DELETE /templates/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{TEMPLATE_TABLE, TemplateRow};

/// Delete `template:<id>`. Returns whether a template existed.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_template(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<TemplateRow> = db
        .delete((TEMPLATE_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
