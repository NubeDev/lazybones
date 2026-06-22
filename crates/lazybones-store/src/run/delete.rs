//! Hard-delete a workflow (run) and the tasks linked to it.
//!
//! Distinct from [`stop_run`](super::stop::stop_run), which keeps the
//! record (flipping `lifecycle` to `stopped`, reversibly) for resume. Delete is
//! the real archive/tombstone path: it removes the
//! run row outright and cascades to its tasks (`run_id = :id`) so no orphaned
//! task is left pointing at a ghost workflow. Each task is removed via
//! [`delete_task`](crate::task::delete_task) so its `depends_on` edges
//! are torn down too. Stopping live agents / refusing to delete running work is
//! the API's job (the delete route); the store just performs the removal.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};
use crate::task::delete_task;

use super::get::get_run;
use super::list::list_run_tasks;
use super::row::{RUN_TABLE, RunRow};

/// Delete `run:<id>` and every task linked to it. Returns whether the run
/// existed.
///
/// The run's tasks are deleted first (each with its `depends_on` edges), then
/// the run row itself.
///
/// # Errors
/// Returns [`StoreError::Operation`] if any task delete or the run delete fails.
pub async fn delete_run(db: &Surreal<Db>, id: &str) -> Result<bool> {
    if get_run(db, id).await?.is_none() {
        return Ok(false);
    }

    for task in list_run_tasks(db, id).await? {
        delete_task(db, &task.id).await?;
    }

    let deleted: Option<RunRow> = db
        .delete((RUN_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;

    Ok(deleted.is_some())
}
