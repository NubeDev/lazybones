//! List asset metadata (`GET /assets`), optionally narrowed by project.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Asset;
use super::row::{ASSET_TABLE, AssetRow};

/// List every asset, optionally narrowed to one `project` scope. Passing `None`
/// lists across all scopes (the only behaviour until projects land).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_assets(db: &Surreal<Db>, project: Option<&str>) -> Result<Vec<Asset>> {
    let mut sql = format!("SELECT * FROM {ASSET_TABLE}");
    if project.is_some() {
        sql.push_str(" WHERE project = $project");
    }
    let rows: Vec<AssetRow> = db
        .query(sql)
        .bind(("project", project.map(ToOwned::to_owned)))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(AssetRow::into_asset).collect())
}
