//! Delete an agent catalog entry by id (`DELETE /agent-catalog/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{AGENT_TABLE, AgentRow};

/// Delete `agent:<id>`. Returns whether an agent existed.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_agent(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<AgentRow> = db
        .delete((AGENT_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
