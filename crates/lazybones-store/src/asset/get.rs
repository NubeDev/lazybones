//! Read a single asset's metadata by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Asset;
use super::row::{ASSET_TABLE, AssetRow};

/// Read `asset:<id>`, or `None` if no such asset exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_asset(db: &Surreal<Db>, id: &str) -> Result<Option<Asset>> {
    let row: Option<AssetRow> = db
        .select((ASSET_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(AssetRow::into_asset))
}
