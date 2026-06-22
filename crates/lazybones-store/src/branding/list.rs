//! List brand profiles (`GET /branding`), optionally narrowed by project.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Branding;
use super::row::{BRANDING_TABLE, BrandingRow};

/// List every brand profile, optionally narrowed to one `project` scope. Passing
/// `None` lists across all scopes (the only behaviour until projects land).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_branding(db: &Surreal<Db>, project: Option<&str>) -> Result<Vec<Branding>> {
    let mut sql = format!("SELECT * FROM {BRANDING_TABLE}");
    if project.is_some() {
        sql.push_str(" WHERE project = $project");
    }
    let rows: Vec<BrandingRow> = db
        .query(sql)
        .bind(("project", project.map(ToOwned::to_owned)))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(BrandingRow::into_branding).collect())
}
