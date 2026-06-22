//! Strict creation of an org (authoring; must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Org;
use super::row::{ORG_TABLE, OrgRow};

/// Create `org` as a new record, failing if its id is already in use.
///
/// Returns the stored org as it is after the write.
///
/// # Errors
/// Returns [`StoreError::OrgExists`] if an org with that id already exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn create_org(db: &Surreal<Db>, org: &Org) -> Result<Org> {
    let existing: Option<OrgRow> = db
        .select((ORG_TABLE, org.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::OrgExists(org.id.clone()));
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
