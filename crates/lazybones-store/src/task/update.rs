//! Edit the seed fields of an existing task (the authoring re-write).
//!
//! Overwrites only the fields a human authors — title/spec/deps/owns/tool/model/effort — and
//! preserves the lifecycle + claim state exactly as [`upsert_task`](super::upsert::upsert_task)
//! does on re-import (SCOPE.md: the DB is authoritative; an edit must not reset a
//! `running` or `done` task). Dep edges are not touched here; the handle diffs
//! old vs new deps and composes `relate_dep`/`unrelate_dep` separately.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::{RetryStrategy, Task, WorktreeMode};
use super::row::{TASK_TABLE, TaskRow};

/// The editable (human-authored) fields of a task.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskEdit {
    /// New human title.
    pub title: String,
    /// New full spec text.
    pub spec: String,
    /// New dependency ids (the handle reconciles the edges).
    pub deps: Vec<String>,
    /// New owned path globs.
    pub owns: Vec<String>,
    /// New per-task agent tool override; `None` falls back to the run config.
    pub tool: Option<String>,
    /// New per-task model override; `None` falls back to the run/global default.
    pub model: Option<String>,
    /// New per-task effort override; `None` falls back to the run/global default.
    pub effort: Option<String>,
    /// New worktree provisioning intent for the loop's next claim.
    pub worktree_mode: WorktreeMode,
    /// Per-task folder-trust auto-seeding override. Outer `Option` is *edit
    /// presence* (`None` = leave unchanged); inner is the value (`Some(None)`
    /// reverts to inheriting the global default, `Some(Some(b))` pins on/off).
    pub auto_trust_agent_folder: Option<Option<bool>>,
    /// The hands-off auto-retry policy. The outer `Option` is *edit presence*
    /// (`None` = leave the policy unchanged); the inner is the value
    /// (`Some(None)` clears it / turns auto-retry off, `Some(Some(s))` sets it).
    pub auto_retry: Option<Option<RetryStrategy>>,
    /// New auto-retry cap; `None` leaves it unchanged.
    pub max_retries: Option<u32>,
    /// Close-on-done policy for the linked issue. `None` = leave unchanged;
    /// `Some(b)` pins it. (The issue *link* itself is managed by the dedicated
    /// create/link/unlink actions, not this authoring edit.)
    pub issue_close_on_done: Option<bool>,
}

/// Overwrite the seed fields of `task:<id>`, preserving status and claim state.
///
/// Returns the updated task as it is after the write.
///
/// # Errors
/// Returns [`StoreError::TaskNotFound`] if no such task exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_task(db: &Surreal<Db>, id: &str, edit: TaskEdit) -> Result<Task> {
    let existing: Option<TaskRow> = db
        .select((TASK_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    let mut to_write = existing
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))?;

    // Refresh only the authored fields; lifecycle + claim state are preserved.
    to_write.title = edit.title;
    to_write.spec = edit.spec;
    to_write.deps = edit.deps;
    to_write.owns = edit.owns;
    to_write.tool = edit.tool;
    to_write.model = edit.model;
    to_write.effort = edit.effort;
    to_write.worktree_mode = edit.worktree_mode;
    // Policy edits are presence-gated: only overwrite when the caller supplied one.
    if let Some(auto_trust) = edit.auto_trust_agent_folder {
        to_write.auto_trust_agent_folder = auto_trust;
    }
    if let Some(auto_retry) = edit.auto_retry {
        to_write.auto_retry = auto_retry;
    }
    if let Some(max_retries) = edit.max_retries {
        to_write.max_retries = max_retries;
    }
    if let Some(close_on_done) = edit.issue_close_on_done {
        to_write.issue_close_on_done = close_on_done;
    }

    let written: Option<TaskRow> = db
        .upsert((TASK_TABLE, id.to_owned()))
        .content(TaskRow::from_task(&to_write))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))
}
