//! Set a run's lifecycle to `stopped` (the human-set pause).
//!
//! This flips only the stored `lifecycle` field. Stopping the live agents and
//! moving the unfinished tasks (reclaim to `ready`, or reset to `pending`) is the
//! API/engine's job (the stop / stop-reset routes); the store just records the
//! decision so `derived_state` reports `stopped` and the scheduler promotes and
//! claims nothing for this run. Reversible: [`resume_run`](super::resume::resume_run)
//! flips it back to `active`.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::get::get_run;
use super::model::{Lifecycle, Run};
use super::row::{RUN_TABLE, RunRow};

/// Mark `run:<id>` stopped. Returns the updated run.
///
/// # Errors
/// Returns [`StoreError::RunNotFound`] if the run does not exist, or
/// [`StoreError::Operation`] if the write fails.
pub async fn stop_run(db: &Surreal<Db>, id: &str) -> Result<Run> {
    set_lifecycle(db, id, Lifecycle::Stopped).await
}

/// Set `run:<id>`'s `lifecycle` and write it back. Shared by stop/resume.
pub(super) async fn set_lifecycle(db: &Surreal<Db>, id: &str, lifecycle: Lifecycle) -> Result<Run> {
    let mut run = get_run(db, id)
        .await?
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))?;
    run.lifecycle = lifecycle;

    let written: Option<RunRow> = db
        .update((RUN_TABLE, id.to_owned()))
        .content(RunRow::from_run(&run))
        .await
        .map_err(StoreError::Operation)?;
    written
        .map(RunRow::into_run)
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))
}
