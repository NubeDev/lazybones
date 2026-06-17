//! The persisted shape of a [`Task`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Task`] carries
//! a plain string id (the concept id). This row maps between the two and stores
//! the lifecycle/claim fields as flat columns so the readiness and filter queries
//! can index `run`/`status` directly.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::Task;
use super::status::Status;

/// The table tasks live in.
pub(crate) const TASK_TABLE: &str = "task";

/// SurrealDB-facing task: the reserved `id` thing plus the task fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct TaskRow {
    pub(crate) id: RecordId,
    pub(crate) run: String,
    pub(crate) title: String,
    pub(crate) spec: String,
    pub(crate) status: String,
    pub(crate) deps: Vec<String>,
    pub(crate) owns: Vec<String>,
    pub(crate) tool: Option<String>,
    pub(crate) session: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) commit: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) heartbeat: Option<String>,
}

impl TaskRow {
    /// Project a domain [`Task`] into its persisted row.
    pub(crate) fn from_task(task: &Task) -> Self {
        Self {
            id: RecordId::new(TASK_TABLE, task.id.as_str()),
            run: task.run.clone(),
            title: task.title.clone(),
            spec: task.spec.clone(),
            status: task.status.as_str().to_owned(),
            deps: task.deps.clone(),
            owns: task.owns.clone(),
            tool: task.tool.clone(),
            session: task.session.clone(),
            worktree: task.worktree.clone(),
            branch: task.branch.clone(),
            commit: task.commit.clone(),
            reason: task.reason.clone(),
            heartbeat: task.heartbeat.clone(),
        }
    }

    /// Reconstruct the domain [`Task`], dropping rows whose status is unknown.
    pub(crate) fn into_task(self) -> Task {
        Task {
            id: task_key(&self.id),
            run: self.run,
            title: self.title,
            spec: self.spec,
            status: parse_status(&self.status),
            deps: self.deps,
            owns: self.owns,
            tool: self.tool,
            session: self.session,
            worktree: self.worktree,
            branch: self.branch,
            commit: self.commit,
            reason: self.reason,
            heartbeat: self.heartbeat,
        }
    }
}

/// The raw string form of a task id's key (the part after `task:`).
fn task_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}

/// Map the stored string back to a [`Status`]; unknown values fall back to
/// `pending` (defensive — the schema only ever writes the canonical strings).
fn parse_status(s: &str) -> Status {
    match s {
        "ready" => Status::Ready,
        "running" => Status::Running,
        "gating" => Status::Gating,
        "done" => Status::Done,
        "blocked" => Status::Blocked,
        _ => Status::Pending,
    }
}
