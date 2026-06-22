//! Auto-retry policy mutators: set a task's hands-off retry policy, and bump its
//! spent-retry counter.
//!
//! These touch only the durable policy/counter fields (`auto_retry`,
//! `max_retries`, `retry_count`) — never the lifecycle status. The status edge
//! itself (the `blocked -> ready` revive) goes through the normal
//! [`Transition::Revive`](super::transition::Transition) path so it stays
//! validated; these verbs just carry the policy bookkeeping alongside it.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::get::get_task;
use super::model::{RetryStrategy, Task};
use super::row::{TASK_TABLE, TaskRow};

/// Set (or clear) a task's auto-retry policy. `strategy = None` turns auto-retry
/// off; `max_retries = None` leaves the existing cap unchanged. Returns the
/// updated task.
///
/// # Errors
/// Returns [`StoreError::TaskNotFound`] if the task is missing, or
/// [`StoreError::Operation`] on a read/write failure.
pub async fn set_retry_policy(
    db: &Surreal<Db>,
    id: &str,
    strategy: Option<RetryStrategy>,
    max_retries: Option<u32>,
) -> Result<Task> {
    let mut task = get_task(db, id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))?;
    task.auto_retry = strategy;
    if let Some(max) = max_retries {
        task.max_retries = max;
    }
    write(db, id, &task).await
}

/// Increment a task's spent-auto-retry counter, returning the updated task.
///
/// # Errors
/// Returns [`StoreError::TaskNotFound`] if the task is missing, or
/// [`StoreError::Operation`] on a read/write failure.
pub async fn bump_retry_count(db: &Surreal<Db>, id: &str) -> Result<Task> {
    let mut task = get_task(db, id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))?;
    task.retry_count = task.retry_count.saturating_add(1);
    write(db, id, &task).await
}

/// Persist `task` back to its row, returning the re-read value.
async fn write(db: &Surreal<Db>, id: &str, task: &Task) -> Result<Task> {
    let written: Option<TaskRow> = db
        .update((TASK_TABLE, id.to_owned()))
        .content(TaskRow::from_task(task))
        .await
        .map_err(StoreError::Operation)?;
    written
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))
}
