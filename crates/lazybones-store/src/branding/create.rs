//! Strict creation of a brand profile (must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Branding;
use super::row::{BRANDING_TABLE, BrandingRow};

/// Create `branding` as a new record, failing if its id is already in use.
///
/// Returns the stored brand as it is after the write.
///
/// # Errors
/// Returns [`StoreError::BrandingExists`] if a brand with that id already exists,
/// or [`StoreError::Operation`] if the read or write fails.
pub async fn create_branding(db: &Surreal<Db>, branding: &Branding) -> Result<Branding> {
    let existing: Option<BrandingRow> = db
        .select((BRANDING_TABLE, branding.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::BrandingExists(branding.id.clone()));
    }

    let written: Option<BrandingRow> = db
        .create((BRANDING_TABLE, branding.id.as_str()))
        .content(BrandingRow::from_branding(branding))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(BrandingRow::into_branding)
        .ok_or_else(|| StoreError::BrandingNotFound(branding.id.clone()))
}
