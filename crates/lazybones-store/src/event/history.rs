//! Read the full event history for a run (`GET /runs/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{EVENT_TABLE, Event, EventRow};

/// Every transition recorded for `run`, oldest first.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn run_history(db: &Surreal<Db>, run: &str) -> Result<Vec<Event>> {
    let rows: Vec<EventRow> = db
        .query(format!(
            "SELECT * FROM {EVENT_TABLE} WHERE run = $run ORDER BY at ASC"
        ))
        .bind(("run", run.to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(EventRow::into_event).collect())
}
