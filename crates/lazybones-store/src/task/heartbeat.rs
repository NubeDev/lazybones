//! Record an agent liveness ping on a running task (no status change).
//!
//! A `running` task whose heartbeat goes stale with no live agent is reclaimable
//! on the next loop pass (SCOPE.md, "Heartbeats"). This write only stamps the
//! time; it never moves the task through the lifecycle.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, SurrealValue};

use crate::error::{Result, StoreError};

use super::row::TASK_TABLE;

/// The single field a heartbeat touches.
#[derive(Debug, Clone, SurrealValue)]
struct HeartbeatPatch {
    heartbeat: String,
}

/// Stamp `task:<id>` with the current time. Returns whether the task exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn record_heartbeat(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let patch = HeartbeatPatch {
        heartbeat: Datetime::now().to_string(),
    };
    let updated: Option<super::row::TaskRow> = db
        .update((TASK_TABLE, id.to_owned()))
        .merge(patch)
        .await
        .map_err(StoreError::Operation)?;
    Ok(updated.is_some())
}
