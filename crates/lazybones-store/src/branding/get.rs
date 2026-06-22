//! Read a single brand profile by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Branding;
use super::row::{BRANDING_TABLE, BrandingRow};

/// Read `branding:<id>`, or `None` if no such brand exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_branding(db: &Surreal<Db>, id: &str) -> Result<Option<Branding>> {
    let row: Option<BrandingRow> = db
        .select((BRANDING_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(BrandingRow::into_branding))
}
