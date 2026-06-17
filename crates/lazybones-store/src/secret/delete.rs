//! Remove a stored secret (the `DELETE /secrets/:tool` write).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{SECRET_TABLE, SecretRow};

/// Delete the credential for `tool`. Returns whether one existed.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_secret(db: &Surreal<Db>, tool: &str) -> Result<bool> {
    let removed: Option<SecretRow> = db
        .delete((SECRET_TABLE, tool))
        .await
        .map_err(StoreError::Operation)?;
    Ok(removed.is_some())
}
