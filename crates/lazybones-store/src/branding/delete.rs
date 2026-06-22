//! Delete a brand profile by id (`DELETE /branding/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{BRANDING_TABLE, BrandingRow};

/// Delete `branding:<id>`. Returns whether a brand existed.
///
/// Note: this does **not** cascade to consumers that stored this `branding_id`
/// (documents, future UI theming) — a reference to a deleted brand simply fails
/// to resolve, and the consumer falls back to the default.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_branding(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<BrandingRow> = db
        .delete((BRANDING_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
