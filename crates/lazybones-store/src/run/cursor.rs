//! Read/advance a run's `hcom_log_cursor` — the persisted hcom-log tail position.
//!
//! Principle 3 (SCOPE.md) forbids in-memory cross-tick state, so how far the tail
//! has drained lives on the [`Run`]. The tail reads the minimum cursor across
//! active runs, drains `id > min`, writes the rows, then advances each touched
//! run's cursor **after** the write — so a crash between the two only re-ingests
//! (idempotent on `(run, hcom_id)`), never skips (docs/hcom-logs-scope.md).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::get::get_run;
use super::model::Run;
use super::row::{RUN_TABLE, RunRow};

/// Advance `run:<id>`'s `hcom_log_cursor` to `max(current, cursor)`. Monotonic:
/// a lower value never moves it backwards. Returns the updated run.
///
/// # Errors
/// Returns [`StoreError::RunNotFound`] if the run does not exist, or
/// [`StoreError::Operation`] if the write fails.
pub async fn advance_hcom_cursor(db: &Surreal<Db>, id: &str, cursor: u64) -> Result<Run> {
    let mut run = get_run(db, id)
        .await?
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))?;
    let next = run.hcom_log_cursor.map_or(cursor, |cur| cur.max(cursor));
    if run.hcom_log_cursor != Some(next) {
        run.hcom_log_cursor = Some(next);
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
