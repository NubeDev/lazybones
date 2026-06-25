//! Apply a validated lifecycle transition and record it in the run log.
//!
//! This is the one write that flips a task's `status`. It refuses any move the
//! [`Status`] state machine forbids ([`StoreError::IllegalTransition`]) and, on a
//! legal move, persists the status plus whatever side-data the transition carries
//! (claim coordinates, commit sha, block reason) and appends an `event` row.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};
use crate::event::{Event, append_event};

use super::get::get_task;
use super::model::Task;
use super::row::{TASK_TABLE, TaskRow};
use super::status::Status;

/// The intent behind a transition, carrying the fields that move with it.
#[derive(Debug, Clone)]
pub enum Transition {
    /// `pending -> ready`: dependencies are met.
    Ready,
    /// `ready -> running`: an agent claimed the task in a worktree.
    Claim {
        /// hcom session id that took the task.
        session: String,
        /// The git worktree path the agent edits.
        worktree: String,
        /// The branch the agent commits to.
        branch: String,
        /// The worktree `HEAD` at claim time (before the agent runs). Recorded so
        /// the empty-task gate can ask "did *this* task advance HEAD?" rather than
        /// the shared-worktree-broken "is the branch ahead of base?". `None` when
        /// the head could not be read (the gate then falls back to the old check).
        base_commit: Option<String>,
    },
    /// `running -> gating`: the agent signalled DONE; gate to re-run.
    Gate,
    /// `gating -> done`: gate re-ran green; commit recorded.
    Done {
        /// The commit sha the agent pushed.
        commit: String,
    },
    /// `* -> blocked`: unrecoverable; reason recorded.
    Block {
        /// Why the task was blocked.
        reason: String,
    },
    /// `running|gating -> ready`: reclaim a stale agent's task.
    Reclaim,
    /// `blocked -> ready`: an operator revived a failed task to workshop it. The
    /// worktree/branch are kept so the next claim resumes in place with the
    /// operator's chat guidance folded into the prompt.
    Revive,
    /// `* -> done`: an **external** event (the linked GitHub issue was closed)
    /// declares the work no longer needed, without a fresh agent commit. Unlike
    /// [`Done`](Self::Done) it carries no commit — the task's existing `commit`
    /// (if any) is preserved — and is reachable from any non-terminal state, so
    /// the reverse-sync poll can land a task whatever stage it is in. This is the
    /// commit-less completion the issue-linkage scope flagged.
    ExternalDone,
}

impl Transition {
    /// The status this transition targets.
    fn target(&self) -> Status {
        match self {
            Transition::Ready | Transition::Reclaim | Transition::Revive => Status::Ready,
            Transition::Claim { .. } => Status::Running,
            Transition::Gate => Status::Gating,
            Transition::Done { .. } | Transition::ExternalDone => Status::Done,
            Transition::Block { .. } => Status::Blocked,
        }
    }
}

/// Move `id` through `transition`, driven by `actor`, recording an event.
///
/// Returns the updated task and the persisted [`Event`] so the caller can both
/// answer the request and publish the transition on the live event bus.
///
/// # Errors
/// Returns [`StoreError::TaskNotFound`] if the task does not exist,
/// [`StoreError::IllegalTransition`] if the current status forbids the move, or
/// [`StoreError::Operation`] if a read/write fails.
pub async fn transition_task(
    db: &Surreal<Db>,
    id: &str,
    transition: Transition,
    actor: &str,
) -> Result<(Task, Event)> {
    let mut task = get_task(db, id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))?;

    let from = task.status;
    let to = transition.target();
    // `ExternalDone` is the one move that doesn't follow the normal edge graph:
    // an external event (issue closed on GitHub) may land a task `done` from any
    // non-terminal state. It is still rejected from a terminal state — a `done`
    // or `blocked` task is not re-completed. Every other transition follows the
    // lifecycle state machine.
    let legal = match transition {
        Transition::ExternalDone => !from.is_terminal(),
        _ => from.can_transition(to),
    };
    if !legal {
        return Err(StoreError::IllegalTransition {
            task: id.to_owned(),
            from: from.as_str().to_owned(),
            to: to.as_str().to_owned(),
        });
    }

    task.status = to;
    let now = surrealdb::types::Datetime::now().to_string();
    apply_side_data(&mut task, &transition, &now);

    let written: Option<TaskRow> = db
        .update((TASK_TABLE, id.to_owned()))
        .content(TaskRow::from_task(&task))
        .await
        .map_err(StoreError::Operation)?;
    let task = written
        .map(TaskRow::into_task)
        .ok_or_else(|| StoreError::TaskNotFound(id.to_owned()))?;

    let event = append_event(db, &task.run, id, from.as_str(), to.as_str(), actor).await?;
    Ok((task, event))
}

/// Fold the transition's side-data into the task before it is written.
///
/// `now` is the RFC3339 instant of this transition (shared with the event row so
/// the task's stamps and its log line never disagree). Lifecycle timing stamps
/// (`started_at`/`finished_at`/`failed_at`) are folded here; durations are
/// derived from them downstream, never stored.
fn apply_side_data(task: &mut Task, transition: &Transition, now: &str) {
    match transition {
        Transition::Claim {
            session,
            worktree,
            branch,
            base_commit,
        } => {
            task.session = Some(session.clone());
            task.worktree = Some(worktree.clone());
            task.branch = Some(branch.clone());
            // Record where the branch sat before this task *first* ran, so the gate
            // can tell a genuine no-op (HEAD never moved past it) from a task that
            // legitimately committed on top of shared work. The caller
            // (`scheduler::tick`) decides the value: it keeps the original baseline
            // across a reclaim/revive onto a reused tree — so a prior attempt's own
            // commit (e.g. green work that died in the reconcile lag) is NOT adopted
            // as the new baseline and the finished work isn't wrongly flagged
            // "empty". We just store whatever it resolved.
            task.base_commit = base_commit.clone();
            // Stamp the first start only; reclaims/revives re-claim the same task
            // and must not reset when work actually began.
            task.started_at.get_or_insert_with(|| now.to_owned());
        }
        Transition::Done { commit } => {
            task.commit = Some(commit.clone());
            task.finished_at = Some(now.to_owned());
            // A task that finishes green is no longer in a failed state.
            task.failed_at = None;
        }
        Transition::ExternalDone => {
            // No fresh commit — keep whatever the task already had (often `None`,
            // since the work was declared complete externally rather than landed).
            task.finished_at = Some(now.to_owned());
            task.failed_at = None;
            // Clear any stale block reason: the task is done, not blocked.
            task.reason = None;
        }
        Transition::Block { reason } => {
            task.reason = Some(reason.clone());
            task.failed_at = Some(now.to_owned());
        }
        Transition::Reclaim => {
            // Drop the dead agent's claim; the worktree is reused on the next pass.
            task.session = None;
        }
        Transition::Revive => {
            // Drop the dead agent's claim and clear the block reason — the task is
            // no longer blocked. The worktree/branch are kept so the next claim
            // resumes in place rather than re-provisioning from scratch. Clearing
            // `failed_at` keeps it reflecting only the latest, still-open failure.
            task.session = None;
            task.reason = None;
            task.failed_at = None;
        }
        Transition::Ready | Transition::Gate => {}
    }
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
    async fn revive_reopens_a_blocked_task_keeping_its_worktree() {
        let db = db().await;
        // A task that failed and was blocked, with a kept worktree for triage.
        let mut t = Task::seed("auth", "wf", "Auth", "spec", vec![], vec![], None);
        t.status = Status::Blocked;
        t.reason = Some("gate failed".into());
        t.session = Some("auth-kula".into());
        t.worktree = Some("/wt/auth".into());
        t.branch = Some("lazy/auth".into());
        create_task(&db, &t).await.unwrap();

        let (out, ev) = transition_task(&db, "auth", Transition::Revive, "operator")
            .await
            .unwrap();
        assert_eq!(out.status, Status::Ready);
        // The dead agent's claim and the block reason are cleared...
        assert_eq!(out.session, None);
        assert_eq!(out.reason, None);
        // ...but the worktree/branch survive so the next claim resumes in place.
        assert_eq!(out.worktree.as_deref(), Some("/wt/auth"));
        assert_eq!(out.branch.as_deref(), Some("lazy/auth"));
        assert_eq!(ev.from, "blocked");
        assert_eq!(ev.to, "ready");
    }

    #[tokio::test]
    async fn lifecycle_stamps_track_start_finish_and_fail() {
        let db = db().await;
        let mut t = Task::seed("a", "wf", "A", "spec", vec![], vec![], None);
        t.status = Status::Ready;
        create_task(&db, &t).await.unwrap();

        // Claim stamps started_at; it stays unset for finish/fail.
        let (claimed, _) = transition_task(
            &db,
            "a",
            Transition::Claim {
                session: "s".into(),
                worktree: "/wt/a".into(),
                branch: "lazy/a".into(),
                base_commit: None,
            },
            "loop",
        )
        .await
        .unwrap();
        let first_start = claimed.started_at.clone();
        assert!(first_start.is_some());
        assert_eq!(claimed.finished_at, None);
        assert_eq!(claimed.failed_at, None);

        // A reclaim back to ready, then re-claim, keeps the original start instant.
        let (_, _) = transition_task(&db, "a", Transition::Reclaim, "loop")
            .await
            .unwrap();
        let (reclaimed, _) = transition_task(
            &db,
            "a",
            Transition::Claim {
                session: "s2".into(),
                worktree: "/wt/a".into(),
                branch: "lazy/a".into(),
                base_commit: None,
            },
            "loop",
        )
        .await
        .unwrap();
        assert_eq!(reclaimed.started_at, first_start, "start is not reset");

        // Gate then Done stamps finished_at and clears any fail.
        transition_task(&db, "a", Transition::Gate, "loop")
            .await
            .unwrap();
        let (done, _) = transition_task(
            &db,
            "a",
            Transition::Done {
                commit: "abc".into(),
            },
            "loop",
        )
        .await
        .unwrap();
        assert!(done.finished_at.is_some());
        assert_eq!(done.failed_at, None);
        assert_eq!(done.started_at, first_start);
    }

    #[tokio::test]
    async fn block_stamps_failed_at_and_revive_clears_it() {
        let db = db().await;
        let mut t = Task::seed("a", "wf", "A", "spec", vec![], vec![], None);
        t.status = Status::Running;
        create_task(&db, &t).await.unwrap();

        let (blocked, _) = transition_task(
            &db,
            "a",
            Transition::Block {
                reason: "gate failed".into(),
            },
            "loop",
        )
        .await
        .unwrap();
        assert!(blocked.failed_at.is_some());

        let (revived, _) = transition_task(&db, "a", Transition::Revive, "operator")
            .await
            .unwrap();
        // Revive clears the failure stamp so it only ever reflects an open failure.
        assert_eq!(revived.failed_at, None);
    }

    #[tokio::test]
    async fn external_done_lands_from_any_live_state_without_a_commit() {
        let db = db().await;
        // A running task, no commit yet — the issue was closed on GitHub.
        let mut t = Task::seed("auth", "wf", "Auth", "spec", vec![], vec![], None);
        t.status = Status::Running;
        create_task(&db, &t).await.unwrap();

        let (done, ev) = transition_task(&db, "auth", Transition::ExternalDone, "issue-sync")
            .await
            .unwrap();
        assert_eq!(done.status, Status::Done);
        // No fresh commit was invented...
        assert_eq!(done.commit, None);
        // ...but it is stamped finished and carries no open failure.
        assert!(done.finished_at.is_some());
        assert_eq!(done.failed_at, None);
        assert_eq!(ev.from, "running");
        assert_eq!(ev.to, "done");
    }

    #[tokio::test]
    async fn external_done_is_rejected_from_a_terminal_state() {
        let db = db().await;
        let mut t = Task::seed("auth", "wf", "Auth", "spec", vec![], vec![], None);
        t.status = Status::Done;
        create_task(&db, &t).await.unwrap();
        let err = transition_task(&db, "auth", Transition::ExternalDone, "issue-sync")
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::IllegalTransition { .. }));
    }

    #[tokio::test]
    async fn revive_is_illegal_from_a_done_task() {
        // `done` is terminal and merged — there is no edge back to `ready`, so a
        // revive is rejected (the chat route turns this into a 409 and points the
        // operator at restart instead). Revive targets `ready`, so it is legal
        // from the other live states (it shares the `* -> ready` edges).
        let db = db().await;
        let mut t = Task::seed("auth", "wf", "Auth", "spec", vec![], vec![], None);
        t.status = Status::Done;
        create_task(&db, &t).await.unwrap();
        let err = transition_task(&db, "auth", Transition::Revive, "operator")
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::IllegalTransition { .. }));
    }
}
