//! Read a single template by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Template;
use super::row::{TEMPLATE_TABLE, TemplateRow};

/// Read `template:<id>`, or `None` if no such template exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_template(db: &Surreal<Db>, id: &str) -> Result<Option<Template>> {
    let row: Option<TemplateRow> = db
        .select((TEMPLATE_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(TemplateRow::into_template))
}
