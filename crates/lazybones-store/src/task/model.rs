//! The durable `Task` document — the full spec lives here, seeded from the
//! workfile and never re-read from disk at runtime (SCOPE.md, Documents).

use serde::{Deserialize, Serialize};

use super::status::Status;

/// How the run loop should provision the working tree when it claims a task.
///
/// This is the operator's *intent*, set at authoring or start time; the loop
/// reads it when claiming. `New` preserves the historical default (an isolated
/// `git worktree add`); the others let a task reuse an existing tree or run on a
/// chosen branch in the main checkout — no per-task worktree at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorktreeMode {
    /// Provision a fresh, isolated worktree on a new branch (the default).
    #[default]
    New,
    /// Reuse an existing worktree at the task's `worktree` path.
    Reuse,
    /// Run in the main checkout on the task's `branch`; create no worktree.
    Branch,
}

/// One unit of work in a run. Keyed by a friendly concept `id` (e.g. `auth`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    /// Friendly concept id, unique within a run (never `WS-01`).
    pub id: String,
    /// The run this task belongs to (groups tasks + history).
    pub run: String,
    /// Human title.
    pub title: String,
    /// Full spec text (inline or imported from `tasks/<id>.md`).
    pub spec: String,
    /// Current lifecycle position.
    pub status: Status,
    /// Ids of tasks that must be `done` before this is `ready`.
    pub deps: Vec<String>,
    /// Optional path globs this task owns (collision guard at merge time).
    pub owns: Vec<String>,
    /// Per-task agent tool override; `None` falls back to the run config.
    pub tool: Option<String>,
    /// How the loop should provision the working tree on claim. Defaults to
    /// `New` (isolated worktree); `#[serde(default)]` keeps tasks stored before
    /// this field readable.
    #[serde(default)]
    pub worktree_mode: WorktreeMode,
    /// The hcom session that claimed this task, if running.
    pub session: Option<String>,
    /// The git worktree path the agent works in, if claimed.
    pub worktree: Option<String>,
    /// The branch the agent commits to, if claimed.
    pub branch: Option<String>,
    /// The commit sha recorded on `done`.
    pub commit: Option<String>,
    /// Why the task was blocked, if blocked.
    pub reason: Option<String>,
    /// RFC3339 timestamp of the agent's last heartbeat, if running.
    pub heartbeat: Option<String>,
}

impl Task {
    /// A freshly imported task: `pending`, no claim state.
    #[must_use]
    pub fn seed(
        id: impl Into<String>,
        run: impl Into<String>,
        title: impl Into<String>,
        spec: impl Into<String>,
        deps: Vec<String>,
        owns: Vec<String>,
        tool: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            run: run.into(),
            title: title.into(),
            spec: spec.into(),
            status: Status::Pending,
            deps,
            owns,
            tool,
            worktree_mode: WorktreeMode::default(),
            session: None,
            worktree: None,
            branch: None,
            commit: None,
            reason: None,
            heartbeat: None,
        }
    }
}
