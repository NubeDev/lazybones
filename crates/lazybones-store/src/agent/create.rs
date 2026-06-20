//! Strict creation of an agent catalog entry (authoring; must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::AgentCatalog;
use super::row::{AGENT_TABLE, AgentRow};

/// Create `agent` as a new record, failing if its id is already in use.
///
/// Returns the stored agent as it is after the write.
///
/// # Errors
/// Returns [`StoreError::AgentExists`] if an agent with that id already exists,
/// or [`StoreError::Operation`] if the read or write fails.
pub async fn create_agent(db: &Surreal<Db>, agent: &AgentCatalog) -> Result<AgentCatalog> {
    let existing: Option<AgentRow> = db
        .select((AGENT_TABLE, agent.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::AgentExists(agent.id.clone()));
    }

    let written: Option<AgentRow> = db
        .create((AGENT_TABLE, agent.id.as_str()))
        .content(AgentRow::from_agent(agent))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(AgentRow::into_agent)
        .ok_or_else(|| StoreError::AgentNotFound(agent.id.clone()))
}
