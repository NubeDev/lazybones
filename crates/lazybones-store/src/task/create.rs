//! Strict creation of a task document (the authoring write that must not clobber).
//!
//! Unlike [`upsert_task`](super::upsert::upsert_task), which reconciles a
//! re-import onto an existing record, this verb is for *authoring* a brand-new
//! task and fails loudly if the concept id is already taken — there is no merge,
//! the caller meant to create something new. Deps are not related here; the
//! handle composes `relate_dep` separately, mirroring the sync path.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Task;
use super::row::{TASK_TABLE, TaskRow};

/// Create `task` as a new record, failing if its id is already in use.
///
/// Returns the stored task as it is after the write.
///
/// # Errors
/// Returns [`StoreError::TaskExists`] if a task with that id already exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn create_task(db: &Surreal<Db>, task: &Task) -> Result<Task> {
    let existing: Option<TaskRow> = db
        .select((TASK_TABLE, task.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::TaskExists(task.id.clone()));
    }

    let written: Option<TaskRow> = db
        .create((TASK_TABLE, task.id.as_str()))
        .content(TaskRow::from_task(task))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(task.id.clone()))
}
