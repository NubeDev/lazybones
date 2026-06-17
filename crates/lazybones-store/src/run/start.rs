//! Stamp a run's `started_at` (the activation timestamp).
//!
//! Promoting the eligible root tasks to `ready` is the API/engine's job (the
//! start route drives the task transitions); the store just records *when* the
//! workflow was activated. Idempotent: re-starting leaves the first timestamp.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::get::get_run;
use super::model::Run;
use super::row::{RUN_TABLE, RunRow};

/// Stamp `run:<id>` with `started_at = now` if not already set. Returns the run.
///
/// # Errors
/// Returns [`StoreError::RunNotFound`] if the run does not exist, or
/// [`StoreError::Operation`] if the write fails.
pub async fn mark_started(db: &Surreal<Db>, id: &str, now: &str) -> Result<Run> {
    let mut run = get_run(db, id)
        .await?
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))?;
    if run.started_at.is_none() {
        run.started_at = Some(now.to_owned());
        let written: Option<RunRow> = db
            .update((RUN_TABLE, id.to_owned()))
            .content(RunRow::from_run(&run))
            .await
            .map_err(StoreError::Operation)?;
        run = written
            .map(RunRow::into_run)
            .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))?;
    }
    Ok(run)
}
