//! Persist a task's GitHub-issue linkage fields (`issue_url`,
//! `issue_close_on_done`, `issue_synced_state`) without disturbing anything else.
//!
//! These three are runtime state owned by the engine's `scheduler::issue`
//! actions and reverse-sync poll — not authored seed fields (so the workfile
//! re-import leaves them alone) and not lifecycle (so they never move `status`).
//! This write reads the row, copies *only* the issue fields off the caller's
//! task, and writes it back, so a concurrent lifecycle change isn't clobbered.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Task;
use super::row::{TASK_TABLE, TaskRow};

/// Write the issue-linkage fields of `task` onto the stored row, preserving its
/// lifecycle and authored fields.
///
/// Returns the stored task as it is after the write.
///
/// # Errors
/// [`StoreError::TaskNotFound`] if the task is gone, [`StoreError::Operation`] on
/// a read/write failure.
pub async fn set_issue_link(db: &Surreal<Db>, task: &Task) -> Result<Task> {
    let existing: Option<TaskRow> = db
        .select((TASK_TABLE, task.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    let mut to_write = existing
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(task.id.clone()))?;

    // Copy only the issue linkage off the caller's task; everything else on the
    // freshly-read row (status, claim, stamps, authored fields) is preserved.
    to_write.issue_url = task.issue_url.clone();
    to_write.issue_close_on_done = task.issue_close_on_done;
    to_write.issue_synced_state = task.issue_synced_state;

    let written: Option<TaskRow> = db
        .upsert((TASK_TABLE, task.id.as_str()))
        .content(TaskRow::from_task(&to_write))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(task.id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;
    use crate::task::create::create_task;
    use crate::task::model::IssueSyncState;
    use crate::task::status::Status;

    async fn db() -> Surreal<Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn set_issue_link_preserves_lifecycle_and_claim() {
        let db = db().await;
        let mut t = Task::seed("auth", "wf", "Auth", "spec", vec![], vec![], None);
        t.status = Status::Running;
        t.session = Some("auth-kula".into());
        t.worktree = Some("/wt/auth".into());
        create_task(&db, &t).await.unwrap();

        // Link an issue via a task carrying *only* the new fields set.
        let mut link = t.clone();
        link.issue_url = Some("https://github.com/o/r/issues/9".into());
        link.issue_close_on_done = true;
        link.issue_synced_state = Some(IssueSyncState::Open);
        // Deliberately scramble a non-issue field to prove it's ignored.
        link.status = Status::Done;
        link.session = None;

        let out = set_issue_link(&db, &link).await.unwrap();
        // Issue fields written...
        assert_eq!(out.issue_url.as_deref(), Some("https://github.com/o/r/issues/9"));
        assert!(out.issue_close_on_done);
        assert_eq!(out.issue_synced_state, Some(IssueSyncState::Open));
        // ...lifecycle + claim preserved from the stored row, not the caller's copy.
        assert_eq!(out.status, Status::Running);
        assert_eq!(out.session.as_deref(), Some("auth-kula"));
        assert_eq!(out.worktree.as_deref(), Some("/wt/auth"));
    }
}
