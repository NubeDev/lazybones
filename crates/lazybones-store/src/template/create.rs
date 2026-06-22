//! Strict creation of a template document (authoring; must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Template;
use super::row::{TEMPLATE_TABLE, TemplateRow};

/// Create `template` as a new record, failing if its id is already in use.
///
/// Returns the stored template as it is after the write.
///
/// # Errors
/// Returns [`StoreError::TemplateExists`] if a template with that id already
/// exists, or [`StoreError::Operation`] if the read or write fails.
pub async fn create_template(db: &Surreal<Db>, template: &Template) -> Result<Template> {
    let existing: Option<TemplateRow> = db
        .select((TEMPLATE_TABLE, template.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::TemplateExists(template.id.clone()));
    }

    let written: Option<TemplateRow> = db
        .create((TEMPLATE_TABLE, template.id.as_str()))
        .content(TemplateRow::from_template(template))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(TemplateRow::into_template)
        .ok_or_else(|| StoreError::TemplateNotFound(template.id.clone()))
}
