//! The **derived** state of a Run — computed from its tasks, never stored.
//!
//! Only `lifecycle` (`active | stopped`) is a human-set, stored field. The
//! user-facing *state* is a pure function of the run's lifecycle and its tasks'
//! statuses, so it can never drift from what the tasks actually are (SCOPE.md
//! principle 6 — the DB is truth; a stored rollup would lie).

use crate::run::model::Lifecycle;
use crate::task::{Status, Task};

/// The derived, user-facing state of a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// Every task is done — the one truly terminal state (besides a hard delete).
    Done,
    /// `lifecycle = stopped`: paused by a human, reversible via resume. Not
    /// terminal — distinct from `Done`, which means the work is actually finished.
    Stopped,
    /// Some task is blocked.
    NeedsAttention,
    /// Some task is running or gating.
    Running,
    /// Some task is ready (a loop could claim it).
    Ready,
    /// No task has been promoted yet (or the run has no tasks).
    Draft,
}

impl RunState {
    /// The lowercase, hyphenated wire form returned over REST.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Done => "done",
            RunState::Stopped => "stopped",
            RunState::NeedsAttention => "needs-attention",
            RunState::Running => "running",
            RunState::Ready => "ready",
            RunState::Draft => "draft",
        }
    }
}

/// Compute a run's state from its `lifecycle` and the statuses of its `tasks`.
///
/// Precedence (most urgent first): done → stopped → needs-attention → running →
/// ready → draft. `done` wins over `stopped` because an all-done run has nothing
/// left to resume — the pause is moot, so it reads as finished, not paused.
/// `done` requires at least one task (an empty run is `draft`, not vacuously
/// done). Otherwise a `stopped` lifecycle overrides the live task states so the
/// UI surfaces the pause (and its Resume affordance).
#[must_use]
pub fn derived_state(lifecycle: Lifecycle, tasks: &[Task]) -> RunState {
    if !tasks.is_empty() && tasks.iter().all(|t| t.status == Status::Done) {
        return RunState::Done;
    }
    if lifecycle == Lifecycle::Stopped {
        return RunState::Stopped;
    }
    if tasks.is_empty() {
        return RunState::Draft;
    }
    if tasks.iter().any(|t| t.status == Status::Blocked) {
        return RunState::NeedsAttention;
    }
    if tasks
        .iter()
        .any(|t| matches!(t.status, Status::Running | Status::Gating))
    {
        return RunState::Running;
    }
    if tasks.iter().any(|t| t.status == Status::Ready) {
        return RunState::Ready;
    }
    RunState::Draft
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(status: Status) -> Task {
        let mut t = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        t.status = status;
        t
    }

    #[test]
    fn stopped_overrides_live_task_states() {
        // A paused run shows `Stopped` over running/blocked/ready so the UI
        // surfaces the pause (and its Resume affordance) rather than the churn.
        assert_eq!(
            derived_state(Lifecycle::Stopped, &[task(Status::Running)]),
            RunState::Stopped
        );
        assert_eq!(
            derived_state(Lifecycle::Stopped, &[task(Status::Blocked)]),
            RunState::Stopped
        );
    }

    #[test]
    fn done_wins_over_stopped() {
        // Nothing left to resume — an all-done run reads finished, not paused.
        assert_eq!(
            derived_state(Lifecycle::Stopped, &[task(Status::Done), task(Status::Done)]),
            RunState::Done
        );
    }

    #[test]
    fn stopped_empty_run_is_stopped() {
        assert_eq!(derived_state(Lifecycle::Stopped, &[]), RunState::Stopped);
    }

    #[test]
    fn empty_run_is_draft() {
        assert_eq!(derived_state(Lifecycle::Active, &[]), RunState::Draft);
    }

    #[test]
    fn all_done_is_done() {
        assert_eq!(
            derived_state(Lifecycle::Active, &[task(Status::Done), task(Status::Done)]),
            RunState::Done
        );
    }

    #[test]
    fn any_blocked_needs_attention() {
        assert_eq!(
            derived_state(
                Lifecycle::Active,
                &[task(Status::Done), task(Status::Blocked), task(Status::Running)]
            ),
            RunState::NeedsAttention
        );
    }

    #[test]
    fn any_running_or_gating_is_running() {
        assert_eq!(
            derived_state(Lifecycle::Active, &[task(Status::Pending), task(Status::Gating)]),
            RunState::Running
        );
        assert_eq!(
            derived_state(Lifecycle::Active, &[task(Status::Running)]),
            RunState::Running
        );
    }

    #[test]
    fn any_ready_is_ready() {
        assert_eq!(
            derived_state(Lifecycle::Active, &[task(Status::Pending), task(Status::Ready)]),
            RunState::Ready
        );
    }

    #[test]
    fn only_pending_is_draft() {
        assert_eq!(
            derived_state(Lifecycle::Active, &[task(Status::Pending)]),
            RunState::Draft
        );
    }
}
