//! Read a Lazybones-Agent configuration record by scope, with global fallback.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::{ManagementAgentConfig, ManagementAgentScope};
use super::row::{ManagementAgentRow, SETTINGS_TABLE};

/// Read the config record stored at exactly `scope`, or `None` if that scope was
/// never configured. Does **not** fall back — use
/// [`get_management_agent_resolved`] for resolution.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_management_agent_scoped(
    db: &Surreal<Db>,
    scope: &ManagementAgentScope,
) -> Result<Option<ManagementAgentConfig>> {
    let row: Option<ManagementAgentRow> = db
        .select((SETTINGS_TABLE, scope.key()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(ManagementAgentRow::into_config))
}

/// The global config record, or `None` if never configured (the common read).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_management_agent(
    db: &Surreal<Db>,
) -> Result<Option<ManagementAgentConfig>> {
    get_management_agent_scoped(db, &ManagementAgentScope::Global).await
}

/// Resolve the effective config for `scope`: the scope's own override if set,
/// else the global record (`workflow-override ?? global`, scope §11 Q1).
///
/// # Errors
/// Returns [`StoreError::Operation`] if a read fails.
pub async fn get_management_agent_resolved(
    db: &Surreal<Db>,
    scope: &ManagementAgentScope,
) -> Result<Option<ManagementAgentConfig>> {
    if let Some(scoped) = get_management_agent_scoped(db, scope).await? {
        return Ok(Some(scoped));
    }
    if matches!(scope, ManagementAgentScope::Global) {
        return Ok(None);
    }
    get_management_agent_scoped(db, &ManagementAgentScope::Global).await
}
