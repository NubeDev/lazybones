//! Force a task back to the start of the lifecycle (`pending`), clearing the
//! runtime state a previous run left behind.
//!
//! This is **not** a [`Transition`](super::transition::Transition): the status
//! state machine has no edge back to `pending` (and `done`/`blocked` are
//! terminal), by design — the normal flow only ever moves forward. A *restart*
//! is a deliberate operator override that resets that machine, so it bypasses
//! `can_transition` and writes `pending` directly. Stopping any live agent and
//! tearing down the worktree is the caller's job (the restart route); this verb
//! only resets the record: status → `pending` and every per-run field
//! (`session`/`worktree`/`branch`/`commit`/`reason`/`heartbeat` and the
//! `started_at`/`finished_at`/`failed_at` timing stamps) cleared. The
//! durable spec (`spec`/`deps`/`owns`/config/links) is preserved so the task
//! re-runs as authored.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};
use crate::event::{Event, append_event};

use super::get::get_task;
use super::model::Task;
use super::row::{TASK_TABLE, TaskRow};
use super::status::Status;

/// Reset `id` to `pending`, clearing its run-specific fields. Returns the updated
/// task and the persisted reset [`Event`] so the caller can publish it on the
/// live bus (mirroring [`transition_task`](super::transition::transition_task)).
///
/// Records a `<from> -> pending` event under `actor` so the restart is visible in
/// the run history. A task already `pending` is reset idempotently (its stale
/// fields are still cleared).
///
/// # Errors
/// Returns [`StoreError::TaskNotFound`] if the task does not exist, or
/// [`StoreError::Operation`] if a read/write fails.
pub async fn reset_task(db: &Surreal<Db>, id: &str, actor: &str) -> Result<(Task, Event)> {
    let mut task = get_task(db, id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))?;

    let from = task.status;
    task.status = Status::Pending;
    // Drop every trace of the prior run; the spec/deps/config/links stay put.
    task.session = None;
    task.worktree = None;
    task.branch = None;
    task.commit = None;
    task.reason = None;
    task.heartbeat = None;
    // A restart is a fresh run: drop the prior run's timing so the new attempt
    // reports its own start/finish/fail, not the old one's.
    task.started_at = None;
    task.finished_at = None;
    task.failed_at = None;
    // A clean (human) reset gives the task a fresh auto-retry budget; the policy
    // itself (`auto_retry`/`max_retries`) is durable config and is preserved.
    task.retry_count = 0;

    let written: Option<TaskRow> = db
        .update((TASK_TABLE, id.to_owned()))
        .content(TaskRow::from_task(&task))
        .await
        .map_err(StoreError::Operation)?;
    let task = written
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))?;

    let event =
        append_event(db, &task.run, id, from.as_str(), Status::Pending.as_str(), actor).await?;
    Ok((task, event))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::use_namespace;
    use crate::connect::{StoreEngine, open_engine};
    use crate::init_schema::init_schema;
    use crate::task::create::create_task;

    async fn db() -> Surreal<Db> {
        let db = open_engine(&StoreEngine::Memory).await.unwrap();
        use_namespace(&db, "lazybones", "test").await.unwrap();
        init_schema(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn reset_clears_run_state_and_returns_to_pending() {
        let db = db().await;
        // A task carrying the full residue of a finished run.
        let mut t = Task::seed("a", "r", "A", "spec", vec!["dep".into()], vec![], None);
        t.status = Status::Done;
        t.session = Some("sess".into());
        t.worktree = Some("/wt/a".into());
        t.branch = Some("lazy/a".into());
        t.commit = Some("abc123".into());
        t.heartbeat = Some("2026-01-01T00:00:00Z".into());
        create_task(&db, &t).await.unwrap();

        let (out, _ev) = reset_task(&db, "a", "operator").await.unwrap();
        assert_eq!(out.status, Status::Pending);
        // Per-run fields cleared...
        assert_eq!(out.session, None);
        assert_eq!(out.worktree, None);
        assert_eq!(out.branch, None);
        assert_eq!(out.commit, None);
        assert_eq!(out.heartbeat, None);
        // ...but the durable spec/deps survive so it re-runs as authored.
        assert_eq!(out.spec, "spec");
        assert_eq!(out.deps, vec!["dep".to_owned()]);
    }

    #[tokio::test]
    async fn reset_missing_task_errors() {
        let db = db().await;
        let err = reset_task(&db, "nope", "operator").await.unwrap_err();
        assert!(matches!(err, StoreError::TaskNotFound(_)));
    }
}
