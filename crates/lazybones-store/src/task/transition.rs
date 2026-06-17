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
}

impl Transition {
    /// The status this transition targets.
    fn target(&self) -> Status {
        match self {
            Transition::Ready | Transition::Reclaim => Status::Ready,
            Transition::Claim { .. } => Status::Running,
            Transition::Gate => Status::Gating,
            Transition::Done { .. } => Status::Done,
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
    if !from.can_transition(to) {
        return Err(StoreError::IllegalTransition {
            task: id.to_owned(),
            from: from.as_str().to_owned(),
            to: to.as_str().to_owned(),
        });
    }

    task.status = to;
    apply_side_data(&mut task, &transition);

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
fn apply_side_data(task: &mut Task, transition: &Transition) {
    match transition {
        Transition::Claim {
            session,
            worktree,
            branch,
        } => {
            task.session = Some(session.clone());
            task.worktree = Some(worktree.clone());
            task.branch = Some(branch.clone());
        }
        Transition::Done { commit } => task.commit = Some(commit.clone()),
        Transition::Block { reason } => task.reason = Some(reason.clone()),
        Transition::Reclaim => {
            // Drop the dead agent's claim; the worktree is reused on the next pass.
            task.session = None;
        }
        Transition::Ready | Transition::Gate => {}
    }
}
