//! List all runs (workflows), and list a single run's tasks.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};
use crate::task::Task;
use crate::task::row::{TASK_TABLE, TaskRow};

use super::model::Run;
use super::row::{RUN_TABLE, RunRow};

/// List every run (workflow).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_runs(db: &Surreal<Db>) -> Result<Vec<Run>> {
    let rows: Vec<RunRow> = db
        .query(format!("SELECT * FROM {RUN_TABLE}"))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(RunRow::into_run).collect())
}

/// List the tasks belonging to run `run_id` (the workflow FK link, not the
/// event-grouping `run` label). Drives the derived run state and task counts.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_run_tasks(db: &Surreal<Db>, run_id: &str) -> Result<Vec<Task>> {
    let rows: Vec<TaskRow> = db
        .query(format!("SELECT * FROM {TASK_TABLE} WHERE run_id = $run_id"))
        .bind(("run_id", run_id.to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(TaskRow::into_task).collect())
}
