//! Strict creation of a run (workflow) document; must not clobber.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Run;
use super::row::{RUN_TABLE, RunRow};

/// Create `run` as a new record, failing if its id is already in use.
///
/// # Errors
/// Returns [`StoreError::RunExists`] if a run with that id already exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn create_run(db: &Surreal<Db>, run: &Run) -> Result<Run> {
    let existing: Option<RunRow> = db
        .select((RUN_TABLE, run.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::RunExists(run.id.clone()));
    }

    let written: Option<RunRow> = db
        .create((RUN_TABLE, run.id.as_str()))
        .content(RunRow::from_run(run))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(RunRow::into_run)
        .ok_or_else(|| StoreError::RunNotFound(run.id.clone()))
}
