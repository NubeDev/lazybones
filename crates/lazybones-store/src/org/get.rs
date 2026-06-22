//! Read a single org by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Org;
use super::row::{ORG_TABLE, OrgRow};

/// Read `org:<id>`, or `None` if no such org exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_org(db: &Surreal<Db>, id: &str) -> Result<Option<Org>> {
    let row: Option<OrgRow> = db
        .select((ORG_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(OrgRow::into_org))
}
