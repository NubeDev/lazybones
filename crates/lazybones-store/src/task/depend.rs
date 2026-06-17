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
/// The edge must be a real graph RELATION (the readiness query traverses
/// `->depends_on->`), so it has to be written with `RELATE`, not `UPSERT` —
/// an `UPSERT`ed row on the relation table is stored as a plain record and the
/// graph traversal rejects it ("not a relation, but expected a RELATION").
///
/// But `RELATE` always *creates*, so re-syncing the same dependency would fail
/// with "record already exists" on the deterministic edge id. We guard it with
/// `IF NOT EXISTS` so the first sync relates and every re-sync is a clean no-op.
///
/// The endpoints are bound as `RecordId` params — SurrealQL wants record-id
/// expressions in the node positions, which `type::thing(..)` is not accepted as
/// inline.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn relate_dep(db: &Surreal<Db>, task: &str, dep: &str) -> Result<()> {
    let from = RecordId::new(TASK_TABLE, task);
    let to = RecordId::new(TASK_TABLE, dep);
    let edge = RecordId::new("depends_on", format!("{task}__{dep}"));
    db.query(
        "IF !(SELECT id FROM ONLY $edge) { RELATE $from->depends_on->$to SET id = $edge }",
    )
    .bind(("edge", edge))
    .bind(("from", from))
    .bind(("to", to))
    .await
    .map_err(StoreError::Operation)?
    .check()
    .map_err(StoreError::Operation)?;
    Ok(())
}

/// Drop the `task ->depends_on-> dep` edge (idempotent: a missing edge is fine).
///
/// The complement of [`relate_dep`] for the authoring path: when an edit removes
/// a dependency, the handle deletes just that one deterministic edge so the
/// readiness traversal stops counting it. Deleting an absent record is a no-op,
/// so calling this for a dep that was never related is safe.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn unrelate_dep(db: &Surreal<Db>, task: &str, dep: &str) -> Result<()> {
    let edge = RecordId::new("depends_on", format!("{task}__{dep}"));
    db.query("DELETE $edge")
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
