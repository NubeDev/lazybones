//! List extension metadata (`GET /extensions`), optionally only the enabled ones.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Extension;
use super::row::{EXTENSION_TABLE, ExtensionRow};

/// List every installed extension. When `enabled_only` is set, only the active
/// ones are returned (what the dispatcher and the frontend loader want — design
/// §4.3 fetches enabled remotes only).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_extensions(db: &Surreal<Db>, enabled_only: bool) -> Result<Vec<Extension>> {
    let mut sql = format!("SELECT * FROM {EXTENSION_TABLE}");
    if enabled_only {
        sql.push_str(" WHERE enabled = true");
    }
    let rows: Vec<ExtensionRow> = db
        .query(sql)
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(ExtensionRow::into_extension).collect())
}
