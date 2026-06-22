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

/// Clear `run:<id>`'s `started_at` (the inverse of [`mark_started`]) and set its
/// lifecycle back to `Active`. Returns the run.
///
/// A *restart* resets every task to `pending` but must also un-activate the run, or
/// the scheduler — which promotes roots for any `Active` run that has a `started_at`
/// — would re-run the workflow immediately, defeating the "press Start when ready"
/// contract. Clearing `started_at` lands the run back in the `draft`-equivalent
/// state the scheduler skips; the next `start` re-stamps it. Lifecycle is forced to
/// `Active` so a restart of a *stopped* run also returns to a startable state.
///
/// # Errors
/// Returns [`StoreError::RunNotFound`] if the run does not exist, or
/// [`StoreError::Operation`] if the write fails.
pub async fn clear_started(db: &Surreal<Db>, id: &str) -> Result<Run> {
    let mut run = get_run(db, id)
        .await?
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))?;
    run.started_at = None;
    run.lifecycle = crate::Lifecycle::Active;
    // A restart re-opens the workflow for another run, so any previously
    // auto-opened PR no longer applies — clear it so completion opens a fresh one.
    run.pr_url = None;
    let written: Option<RunRow> = db
        .update((RUN_TABLE, id.to_owned()))
        .content(RunRow::from_run(&run))
        .await
        .map_err(StoreError::Operation)?;
    written
        .map(RunRow::into_run)
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))
}

/// Record the auto-opened PR `url` on `run:<id>` (the idempotency guard for the
/// auto-PR flow: once set, the engine won't open another). Returns the run.
///
/// # Errors
/// Returns [`StoreError::RunNotFound`] if the run does not exist, or
/// [`StoreError::Operation`] if the write fails.
pub async fn set_pr_url(db: &Surreal<Db>, id: &str, url: &str) -> Result<Run> {
    let mut run = get_run(db, id)
        .await?
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))?;
    run.pr_url = Some(url.to_owned());
    let written: Option<RunRow> = db
        .update((RUN_TABLE, id.to_owned()))
        .content(RunRow::from_run(&run))
        .await
        .map_err(StoreError::Operation)?;
    written
        .map(RunRow::into_run)
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))
}
