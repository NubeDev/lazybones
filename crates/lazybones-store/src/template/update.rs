//! Edit an existing template document, preserving `created_at`.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Template;
use super::row::{TEMPLATE_TABLE, TemplateRow};

/// Overwrite the editable fields of `template:<id>`, failing if no such template
/// exists. The original `created_at` is preserved; `template.updated_at` is
/// stored as the new update stamp.
///
/// Returns the stored template as it is after the write.
///
/// # Errors
/// Returns [`StoreError::TemplateNotFound`] if no template with that id exists,
/// or [`StoreError::Operation`] if the read or write fails.
pub async fn update_template(db: &Surreal<Db>, template: &Template) -> Result<Template> {
    let existing: Option<TemplateRow> = db
        .select((TEMPLATE_TABLE, template.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    let Some(existing) = existing else {
        return Err(StoreError::TemplateNotFound(template.id.clone()));
    };

    // Preserve the immutable creation stamp regardless of what the caller sent.
    let mut row = TemplateRow::from_template(template);
    row.created_at = existing.created_at;

    let written: Option<TemplateRow> = db
        .update((TEMPLATE_TABLE, template.id.as_str()))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(TemplateRow::into_template)
        .ok_or_else(|| StoreError::TemplateNotFound(template.id.clone()))
}
