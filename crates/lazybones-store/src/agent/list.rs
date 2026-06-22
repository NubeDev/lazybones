//! List all agent catalog entries (`GET /agent-catalog`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::AgentCatalog;
use super::row::{AGENT_TABLE, AgentRow};

/// List every agent catalog entry, ordered by id for a stable UI.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_agents(db: &Surreal<Db>) -> Result<Vec<AgentCatalog>> {
    let rows: Vec<AgentRow> = db
        .query(format!("SELECT * FROM {AGENT_TABLE} ORDER BY id"))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(AgentRow::into_agent).collect())
}
