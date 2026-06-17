//! List tasks, optionally narrowed by status (`GET /tasks?status=ready`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Task;
use super::row::{TASK_TABLE, TaskRow};
use super::status::Status;

/// List every task, or only those in `status` when given.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_tasks(db: &Surreal<Db>, status: Option<Status>) -> Result<Vec<Task>> {
    let rows: Vec<TaskRow> = match status {
        Some(s) => db
            .query(format!("SELECT * FROM {TASK_TABLE} WHERE status = $status"))
            .bind(("status", s.as_str().to_owned()))
            .await
            .map_err(StoreError::Operation)?
            .take(0)
            .map_err(StoreError::Operation)?,
        None => db
            .query(format!("SELECT * FROM {TASK_TABLE}"))
            .await
            .map_err(StoreError::Operation)?
            .take(0)
            .map_err(StoreError::Operation)?,
    };
    Ok(rows.into_iter().map(TaskRow::into_task).collect())
}
