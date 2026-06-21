//! Idempotent upsert of a task document (the `POST /workfile/sync` write).
//!
//! Re-syncing an existing task overwrites its spec/title/deps/owns but preserves
//! its lifecycle: a task already `running` or `done` is not reset to `pending` by
//! a re-import (SCOPE.md: the DB is authoritative; re-import reconciles).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Task;
use super::row::{TASK_TABLE, TaskRow};

/// Insert `task` if absent, or update its seed fields if present.
///
/// Returns the stored task as it is after the write.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails.
pub async fn upsert_task(db: &Surreal<Db>, task: &Task) -> Result<Task> {
    let existing: Option<TaskRow> = db
        .select((TASK_TABLE, task.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;

    // Preserve lifecycle + claim state on re-import; only refresh seed fields.
    let mut to_write = task.clone();
    if let Some(prev) = existing {
        let prev = prev.into_task();
        to_write.status = prev.status;
        to_write.session = prev.session;
        to_write.worktree = prev.worktree;
        to_write.branch = prev.branch;
        to_write.commit = prev.commit;
        to_write.reason = prev.reason;
        // Issue linkage is runtime state set by the create/link/unlink actions
        // and the reverse-sync poll — not an authored seed field — so a
        // re-import (which carries no issue fields) must not clear it.
        to_write.issue_url = prev.issue_url;
        to_write.issue_close_on_done = prev.issue_close_on_done;
        to_write.issue_synced_state = prev.issue_synced_state;
    }

    let written: Option<TaskRow> = db
        .upsert((TASK_TABLE, task.id.as_str()))
        .content(TaskRow::from_task(&to_write))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(task.id.clone()))
}
