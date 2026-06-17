//! List all task templates (`GET /templates`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Template;
use super::row::{TEMPLATE_TABLE, TemplateRow};

/// List every template.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_templates(db: &Surreal<Db>) -> Result<Vec<Template>> {
    let rows: Vec<TemplateRow> = db
        .query(format!("SELECT * FROM {TEMPLATE_TABLE}"))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(TemplateRow::into_template).collect())
}
