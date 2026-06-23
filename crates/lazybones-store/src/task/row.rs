//! The persisted shape of a [`Task`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Task`] carries
//! a plain string id (the concept id). This row maps between the two and stores
//! the lifecycle/claim fields as flat columns so the readiness and filter queries
//! can index `run`/`status` directly.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::{DEFAULT_MAX_RETRIES, IssueSyncState, RetryStrategy, Task, WorktreeMode};
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
    /// Per-task override of folder-trust auto-seeding; `None` inherits the global
    /// default (on). `Option` so rows written before this column read back as
    /// `None` (= inherit), needing no migration.
    pub(crate) auto_trust_agent_folder: Option<bool>,
    /// Worktree provisioning intent, stored as its lowercase string form.
    /// `Option` so rows written before this column read back as the default.
    pub(crate) worktree_mode: Option<String>,
    pub(crate) session: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) branch: Option<String>,
    /// Worktree HEAD captured at claim; `Option` so legacy rows read back as
    /// `None` (the empty-task gate then falls back to the old branch-ahead check).
    pub(crate) base_commit: Option<String>,
    pub(crate) commit: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) heartbeat: Option<String>,
    /// Lifecycle timing stamps (RFC3339); `Option` so legacy rows read back as
    /// `None`. `started_at` = first claim, `finished_at` = done, `failed_at` =
    /// most recent block (cleared on revive). Durations are derived, not stored.
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) failed_at: Option<String>,
    /// FK to the parent workflow run; `Option` so legacy rows read back as `None`.
    pub(crate) run_id: Option<String>,
    /// Provenance template id; `Option` for forward/backward compatibility.
    pub(crate) template_id: Option<String>,
    /// Reuse-tree source task id; `Option` for forward/backward compatibility.
    pub(crate) reuse_from: Option<String>,
    /// Workflow-only worktree-mode override, stored as its lowercase string form.
    /// `Option` so legacy rows (and standalone tasks) read back as `None`.
    pub(crate) worktree_mode_override: Option<String>,
    /// Auto-retry strategy (`long_term`/`quick`), stored as its string form.
    /// `Option` (and `None` = off) so legacy rows read back with auto-retry off.
    pub(crate) auto_retry: Option<String>,
    /// Auto-retry cap; `Option` so legacy rows read back as the default.
    pub(crate) max_retries: Option<u32>,
    /// Auto-retries spent; `Option` so legacy rows read back as 0.
    pub(crate) retry_count: Option<u32>,
    /// Linked GitHub issue URL; `None`/absent = unlinked. `Option` so legacy
    /// rows read back unlinked, needing no migration.
    pub(crate) issue_url: Option<String>,
    /// Close-on-done flag; `Option` so legacy rows read back as `false`.
    pub(crate) issue_close_on_done: Option<bool>,
    /// Last-synced issue state (`open`/`closed`), stored as its string form.
    /// `Option` (and `None` = never synced) so legacy rows read back unsynced.
    pub(crate) issue_synced_state: Option<String>,
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
            auto_trust_agent_folder: task.auto_trust_agent_folder,
            worktree_mode: Some(task.worktree_mode.as_str().to_owned()),
            session: task.session.clone(),
            worktree: task.worktree.clone(),
            branch: task.branch.clone(),
            base_commit: task.base_commit.clone(),
            commit: task.commit.clone(),
            reason: task.reason.clone(),
            heartbeat: task.heartbeat.clone(),
            started_at: task.started_at.clone(),
            finished_at: task.finished_at.clone(),
            failed_at: task.failed_at.clone(),
            run_id: task.run_id.clone(),
            template_id: task.template_id.clone(),
            reuse_from: task.reuse_from.clone(),
            worktree_mode_override: task.worktree_mode_override.map(|m| m.as_str().to_owned()),
            auto_retry: task.auto_retry.map(|s| s.as_str().to_owned()),
            max_retries: Some(task.max_retries),
            retry_count: Some(task.retry_count),
            issue_url: task.issue_url.clone(),
            issue_close_on_done: Some(task.issue_close_on_done),
            issue_synced_state: task.issue_synced_state.map(|s| s.as_str().to_owned()),
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
            auto_trust_agent_folder: self.auto_trust_agent_folder,
            worktree_mode: WorktreeMode::parse(self.worktree_mode.as_deref()),
            session: self.session,
            worktree: self.worktree,
            branch: self.branch,
            base_commit: self.base_commit,
            commit: self.commit,
            reason: self.reason,
            heartbeat: self.heartbeat,
            started_at: self.started_at,
            finished_at: self.finished_at,
            failed_at: self.failed_at,
            run_id: self.run_id,
            template_id: self.template_id,
            reuse_from: self.reuse_from,
            // `None` stays `None` (inherit); only a stored value parses to Some.
            worktree_mode_override: self
                .worktree_mode_override
                .as_deref()
                .map(|s| WorktreeMode::parse(Some(s))),
            auto_retry: RetryStrategy::parse(self.auto_retry.as_deref()),
            max_retries: self.max_retries.unwrap_or(DEFAULT_MAX_RETRIES),
            retry_count: self.retry_count.unwrap_or(0),
            issue_url: self.issue_url,
            issue_close_on_done: self.issue_close_on_done.unwrap_or(false),
            issue_synced_state: IssueSyncState::parse(self.issue_synced_state.as_deref()),
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
