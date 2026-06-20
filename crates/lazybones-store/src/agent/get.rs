//! Read a single agent catalog entry by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::AgentCatalog;
use super::row::{AGENT_TABLE, AgentRow};

/// Read `agent:<id>`, or `None` if no such agent exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_agent(db: &Surreal<Db>, id: &str) -> Result<Option<AgentCatalog>> {
    let row: Option<AgentRow> = db
        .select((AGENT_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(AgentRow::into_agent))
}
