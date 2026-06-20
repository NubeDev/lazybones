//! The persisted shape of a [`Task`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Task`] carries
//! a plain string id (the concept id). This row maps between the two and stores
//! the lifecycle/claim fields as flat columns so the readiness and filter queries
//! can index `run`/`status` directly.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::{Task, WorktreeMode};
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
    /// Per-task model + effort forwarded to the agent CLI; `Option` so rows
    /// written before these columns read back as `None`.
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    /// Worktree provisioning intent, stored as its lowercase string form.
    /// `Option` so rows written before this column read back as the default.
    pub(crate) worktree_mode: Option<String>,
    pub(crate) session: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) commit: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) heartbeat: Option<String>,
    /// FK to the parent workflow run; `Option` so legacy rows read back as `None`.
    pub(crate) run_id: Option<String>,
    /// Provenance template id; `Option` for forward/backward compatibility.
    pub(crate) template_id: Option<String>,
    /// Reuse-tree source task id; `Option` for forward/backward compatibility.
    pub(crate) reuse_from: Option<String>,
    /// Workflow-only worktree-mode override, stored as its lowercase string form.
    /// `Option` so legacy rows (and standalone tasks) read back as `None`.
    pub(crate) worktree_mode_override: Option<String>,
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
            model: task.model.clone(),
            effort: task.effort.clone(),
            worktree_mode: Some(task.worktree_mode.as_str().to_owned()),
            session: task.session.clone(),
            worktree: task.worktree.clone(),
            branch: task.branch.clone(),
            commit: task.commit.clone(),
            reason: task.reason.clone(),
            heartbeat: task.heartbeat.clone(),
            run_id: task.run_id.clone(),
            template_id: task.template_id.clone(),
            reuse_from: task.reuse_from.clone(),
            worktree_mode_override: task.worktree_mode_override.map(|m| m.as_str().to_owned()),
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
            model: self.model,
            effort: self.effort,
            worktree_mode: WorktreeMode::parse(self.worktree_mode.as_deref()),
            session: self.session,
            worktree: self.worktree,
            branch: self.branch,
            commit: self.commit,
            reason: self.reason,
            heartbeat: self.heartbeat,
            run_id: self.run_id,
            template_id: self.template_id,
            reuse_from: self.reuse_from,
            // `None` stays `None` (inherit); only a stored value parses to Some.
            worktree_mode_override: self
                .worktree_mode_override
                .as_deref()
                .map(|s| WorktreeMode::parse(Some(s))),
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
