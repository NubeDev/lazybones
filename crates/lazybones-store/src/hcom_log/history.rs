//! Read a run's durable hcom log (`GET /runs/:id/hcom`), oldest first.
//!
//! Modelled on [`run_history`](crate::Event)'s query but over `hcom_log`, with the
//! optional `task`/`kind`/`after`/`limit` filters the REST surface exposes.
//! Ordering is by `hcom_id` (hcom's monotonic event id), the same total order the
//! ingestion cursor advances along.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{HCOM_LOG_TABLE, HcomLogEntry, HcomLogRow};

/// Filters for an hcom-log read. All `None` reads the whole run's log.
#[derive(Debug, Clone, Default)]
pub struct HcomLogFilter {
    /// Restrict to one task's agent (the `?task=<id>` query).
    pub task: Option<String>,
    /// Restrict to one event kind (`message | status | life`).
    pub kind: Option<String>,
    /// Page boundary: only events with `hcom_id > after`.
    pub after: Option<i64>,
    /// Page size cap.
    pub limit: Option<usize>,
}

/// Every hcom event recorded for `run`, oldest first, narrowed by `filter`.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn run_hcom_log(
    db: &Surreal<Db>,
    run: &str,
    filter: &HcomLogFilter,
) -> Result<Vec<HcomLogEntry>> {
    let mut sql = format!("SELECT * FROM {HCOM_LOG_TABLE} WHERE run = $run");
    if filter.task.is_some() {
        sql.push_str(" AND task = $task");
    }
    if filter.kind.is_some() {
        sql.push_str(" AND kind = $kind");
    }
    if filter.after.is_some() {
        sql.push_str(" AND hcom_id > $after");
    }
    sql.push_str(" ORDER BY hcom_id ASC");
    if let Some(limit) = filter.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }

    let mut q = db.query(sql).bind(("run", run.to_owned()));
    if let Some(task) = &filter.task {
        q = q.bind(("task", task.clone()));
    }
    if let Some(kind) = &filter.kind {
        q = q.bind(("kind", kind.clone()));
    }
    if let Some(after) = filter.after {
        q = q.bind(("after", after));
    }

    let rows: Vec<HcomLogRow> = q
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(HcomLogRow::into_entry).collect())
}
