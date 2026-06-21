//! Read and resolve follow-ups for a run.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, RecordId};

use crate::error::{Result, StoreError};

use super::row::{FOLLOW_UP_TABLE, FollowUp, FollowUpRow};

/// Filters for [`run_follow_ups`].
#[derive(Debug, Clone, Default)]
pub struct FollowUpFilter {
    /// Restrict to one status (`open` | `resolved`); `None` returns both.
    pub status: Option<String>,
    /// Restrict to one task's follow-ups.
    pub task: Option<String>,
}

/// A run's follow-ups, most-recently-updated first (the freshest wall on top).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn run_follow_ups(
    db: &Surreal<Db>,
    run: &str,
    filter: &FollowUpFilter,
) -> Result<Vec<FollowUp>> {
    let mut sql =
        format!("SELECT * FROM {FOLLOW_UP_TABLE} WHERE run = $run");
    if filter.status.is_some() {
        sql.push_str(" AND status = $status");
    }
    if filter.task.is_some() {
        sql.push_str(" AND task = $task");
    }
    sql.push_str(" ORDER BY updated_at DESC");

    let rows: Vec<FollowUpRow> = db
        .query(sql)
        .bind(("run", run.to_owned()))
        .bind(("status", filter.status.clone().unwrap_or_default()))
        .bind(("task", filter.task.clone().unwrap_or_default()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(FollowUpRow::into_follow_up).collect())
}

/// Mark one follow-up `resolved` by its row key. Returns the updated
/// [`FollowUp`], or `None` if no row has that id. Idempotent: resolving an
/// already-resolved follow-up just refreshes its `resolved_at`.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn resolve_follow_up(db: &Surreal<Db>, id: &str) -> Result<Option<FollowUp>> {
    let record = RecordId::new(FOLLOW_UP_TABLE, id);
    let updated: Option<FollowUpRow> = db
        .query(
            "UPDATE $id SET status = 'resolved', resolved_at = $now, updated_at = $now \
             RETURN AFTER",
        )
        .bind(("id", record))
        .bind(("now", Datetime::now()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(updated.map(FollowUpRow::into_follow_up))
}
