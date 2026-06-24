//! Idempotent creation of an org (cloud-authored, single-writer).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Org;
use super::row::{ORG_TABLE, OrgRow};

/// Ensure `org:<id>` exists, returning it. The org graph is cloud-authored and
/// single-writer (decisions §3), so create is idempotent: re-creating an existing
/// id returns the stored record rather than erroring.
///
/// # Errors
/// Returns [`StoreError::OrgNotFound`] if the write reports no row, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn create_org(db: &Surreal<Db>, org: &Org) -> Result<Org> {
    let existing: Option<OrgRow> = db
        .select((ORG_TABLE, org.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if let Some(row) = existing {
        return Ok(row.into_org());
    }

    let written: Option<OrgRow> = db
        .create((ORG_TABLE, org.id.as_str()))
        .content(OrgRow::from_org(org))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(OrgRow::into_org)
        .ok_or_else(|| StoreError::OrgNotFound(org.id.clone()))
}
