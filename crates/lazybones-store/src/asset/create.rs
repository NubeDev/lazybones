//! Content-addressed creation of an asset metadata row (dedups on sha256).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Asset;
use super::row::{ASSET_TABLE, AssetRow};

/// Create the metadata row for `asset`, **content-addressed**: if a row with the
/// same `sha256` (within the same `project` scope) already exists, that existing
/// asset is returned unchanged rather than a duplicate created. This is what makes
/// re-uploading the same bytes dedup to one reusable asset.
///
/// Returns the stored (or pre-existing) asset.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails, or
/// [`StoreError::AssetNotFound`] if the insert unexpectedly returns no row.
pub async fn create_asset(db: &Surreal<Db>, asset: &Asset) -> Result<Asset> {
    // Dedup on (sha256, project). The `asset_sha256` index makes the lookup cheap;
    // we narrow by project in Rust so a `None` scope matches `None` exactly.
    let same_hash: Vec<AssetRow> = db
        .query(format!("SELECT * FROM {ASSET_TABLE} WHERE sha256 = $sha"))
        .bind(("sha", asset.sha256.clone()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    if let Some(existing) = same_hash
        .into_iter()
        .map(AssetRow::into_asset)
        .find(|a| a.project == asset.project)
    {
        return Ok(existing);
    }

    let written: Option<AssetRow> = db
        .create((ASSET_TABLE, asset.id.as_str()))
        .content(AssetRow::from_asset(asset))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(AssetRow::into_asset)
        .ok_or_else(|| StoreError::AssetNotFound(asset.id.clone()))
}
