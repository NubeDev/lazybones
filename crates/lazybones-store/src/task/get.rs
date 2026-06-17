//! Read a single task by its concept id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Task;
use super::row::{TASK_TABLE, TaskRow};

/// Read `task:<id>`, or `None` if no such task exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_task(db: &Surreal<Db>, id: &str) -> Result<Option<Task>> {
    let row: Option<TaskRow> = db
        .select((TASK_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(TaskRow::into_task))
}
