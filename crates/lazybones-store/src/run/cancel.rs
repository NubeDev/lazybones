//! Set a run's lifecycle to `cancelled` (the human-set state change).
//!
//! This flips only the stored `lifecycle` field. Stopping the live agents and
//! blocking the unclaimed tasks is the API/engine's job (the cancel route); the
//! store just records the decision so `derived_state` reports `cancelled`.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::get::get_run;
use super::model::{Lifecycle, Run};
use super::row::{RUN_TABLE, RunRow};

/// Mark `run:<id>` cancelled. Returns the updated run.
///
/// # Errors
/// Returns [`StoreError::RunNotFound`] if the run does not exist, or
/// [`StoreError::Operation`] if the write fails.
pub async fn cancel_run(db: &Surreal<Db>, id: &str) -> Result<Run> {
    let mut run = get_run(db, id)
        .await?
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))?;
    run.lifecycle = Lifecycle::Cancelled;

    let written: Option<RunRow> = db
        .update((RUN_TABLE, id.to_owned()))
        .content(RunRow::from_run(&run))
        .await
        .map_err(StoreError::Operation)?;
    written
        .map(RunRow::into_run)
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))
}
