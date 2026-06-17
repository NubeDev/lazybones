//! Dependency graph edges (`task ->depends_on-> task`) and the readiness query.
//!
//! Readiness is graph-driven (SCOPE.md, Graph): a `pending` task becomes `ready`
//! when every task it `depends_on` is `done`. The edges are written on sync from
//! each task's `deps`; the readiness pass reads them back.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{RecordId, SurrealValue};

use crate::error::{Result, StoreError};

use super::row::TASK_TABLE;

/// Relate `task ->depends_on-> dep` (idempotent: re-relating is a no-op).
///
/// The edge endpoints are bound as `RecordId` params — SurrealQL's `RELATE` wants
/// record-id expressions in the node positions, which `type::thing(..)` is not
/// accepted as inline. A deterministic edge id (`[task, dep]`) makes the relate
/// idempotent: re-syncing the same dependency overwrites the same edge.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn relate_dep(db: &Surreal<Db>, task: &str, dep: &str) -> Result<()> {
    let from = RecordId::new(TASK_TABLE, task);
    let to = RecordId::new(TASK_TABLE, dep);
    let edge = RecordId::new("depends_on", format!("{task}__{dep}"));
    db.query("RELATE $from->depends_on->$to SET id = $edge")
        .bind(("from", from))
        .bind(("to", to))
        .bind(("edge", edge))
        .await
        .map_err(StoreError::Operation)?
        .check()
        .map_err(StoreError::Operation)?;
    Ok(())
}

/// A task id paired with whether all of its dependencies are `done`.
#[derive(Debug, Clone, SurrealValue)]
struct Readiness {
    id: String,
    ready: bool,
}

/// The concept ids of `pending` tasks whose every dependency is `done`.
///
/// A task with no dependencies is ready immediately. Runs over the `depends_on`
/// graph so the answer reflects the current status of each dependency.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn newly_ready(db: &Surreal<Db>) -> Result<Vec<String>> {
    let rows: Vec<Readiness> = db
        .query(format!(
            "SELECT meta::id(id) AS id, \
             array::all((->depends_on->task.status), |$s| $s = 'done') AS ready \
             FROM {TASK_TABLE} WHERE status = 'pending'"
        ))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().filter(|r| r.ready).map(|r| r.id).collect())
}
