//! Upsert / delete a Lazybones-Agent configuration record by scope.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::{ManagementAgentConfig, ManagementAgentScope};
use super::row::{ManagementAgentRow, SETTINGS_TABLE};

/// Write `config` at `scope`, returning it as stored. Idempotent on the scope
/// key: writing again overwrites that scope's record in place.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn put_management_agent_scoped(
    db: &Surreal<Db>,
    scope: &ManagementAgentScope,
    config: &ManagementAgentConfig,
) -> Result<ManagementAgentConfig> {
    let key = scope.key();
    let written: Option<ManagementAgentRow> = db
        .upsert((SETTINGS_TABLE, key))
        .content(ManagementAgentRow::from_config(&scope.key(), config))
        .await
        .map_err(StoreError::Operation)?;

    written.map(ManagementAgentRow::into_config).ok_or_else(|| {
        StoreError::Operation(surrealdb::Error::thrown(
            "management agent config vanished after write".to_owned(),
        ))
    })
}

/// Write the global config record (the common write).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn put_management_agent(
    db: &Surreal<Db>,
    config: &ManagementAgentConfig,
) -> Result<ManagementAgentConfig> {
    put_management_agent_scoped(db, &ManagementAgentScope::Global, config).await
}

/// Delete the config record at `scope`, returning whether it existed. Removing a
/// workflow override reverts that workflow to the global default.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_management_agent_scoped(
    db: &Surreal<Db>,
    scope: &ManagementAgentScope,
) -> Result<bool> {
    let deleted: Option<ManagementAgentRow> = db
        .delete((SETTINGS_TABLE, scope.key()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
