//! List all orgs (`GET /orgs`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Org;
use super::row::{ORG_TABLE, OrgRow};

/// List every org.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_orgs(db: &Surreal<Db>) -> Result<Vec<Org>> {
    let rows: Vec<OrgRow> = db
        .query(format!("SELECT * FROM {ORG_TABLE}"))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(OrgRow::into_org).collect())
}
