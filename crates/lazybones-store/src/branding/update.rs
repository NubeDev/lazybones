//! Edit an existing brand profile, preserving `created_at`.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Branding;
use super::row::{BRANDING_TABLE, BrandingRow};

/// Overwrite the editable fields of `branding:<id>`, failing if no such brand
/// exists. The original `created_at` is preserved; `branding.updated_at` is
/// stored as the new update stamp.
///
/// Returns the stored brand as it is after the write.
///
/// # Errors
/// Returns [`StoreError::BrandingNotFound`] if no brand with that id exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_branding(db: &Surreal<Db>, branding: &Branding) -> Result<Branding> {
    let existing: Option<BrandingRow> = db
        .select((BRANDING_TABLE, branding.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    let Some(existing) = existing else {
        return Err(StoreError::BrandingNotFound(branding.id.clone()));
    };

    // Preserve the immutable creation stamp regardless of what the caller sent.
    let mut row = BrandingRow::from_branding(branding);
    row.created_at = existing.created_at;

    let written: Option<BrandingRow> = db
        .update((BRANDING_TABLE, branding.id.as_str()))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(BrandingRow::into_branding)
        .ok_or_else(|| StoreError::BrandingNotFound(branding.id.clone()))
}
