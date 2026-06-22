//! The persisted shape of a [`Run`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`. The workspace is stored as a
//! flat sub-object; `Option` columns keep the row forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use crate::task::WorktreeMode;

use super::model::{Lifecycle, MergeMode, Run, Workspace};

/// The table runs (workflows) live in. The public path is `/workflows`, but the
/// table stays `run` to match docs/starting-workflows.md.
pub(crate) const RUN_TABLE: &str = "run";

/// SurrealDB-facing workspace sub-object.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct WorkspaceRow {
    pub(crate) repo: String,
    pub(crate) base_branch: Option<String>,
    pub(crate) branch_prefix: Option<String>,
    pub(crate) worktree_mode: Option<String>,
    /// Default agent triple for the workflow's tasks; `Option` so rows written
    /// before these columns read back as `None` (inherit the global).
    pub(crate) tool: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    /// Per-workflow gate commands; `None` inherits the global, `Some([])` = no gate.
    /// `Option` so rows written before this column read back as `None`.
    pub(crate) gate: Option<Vec<String>>,
    /// Per-workflow merge strategy (`fast-forward` | `merge` | `pr`); `None`
    /// inherits the global. `Option` so pre-column rows read back as `None`.
    pub(crate) merge: Option<String>,
    /// Open a PR automatically when every task is done. `Option` so pre-column
    /// rows read back as `None` (off).
    pub(crate) auto_pr: Option<bool>,
}

/// SurrealDB-facing run: the reserved `id` thing plus the workflow fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct RunRow {
    pub(crate) id: RecordId,
    pub(crate) title: String,
    pub(crate) workspace: WorkspaceRow,
    pub(crate) lifecycle: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) started_at: Option<String>,
    /// hcom-log ingestion cursor (the highest `hcom_id` drained for this run).
    /// Stored as `int` (`i64`); hcom ids are positive monotonic so they fit.
    /// `Option` so rows written before this column read back as `None`.
    pub(crate) hcom_log_cursor: Option<i64>,
    /// The auto-opened PR url, once the run completed and the PR was created.
    /// `Option` so pre-column rows read back as `None`.
    pub(crate) pr_url: Option<String>,
}

impl RunRow {
    /// Project a domain [`Run`] into its persisted row.
    pub(crate) fn from_run(run: &Run) -> Self {
        Self {
            id: RecordId::new(RUN_TABLE, run.id.as_str()),
            title: run.title.clone(),
            workspace: WorkspaceRow {
                repo: run.workspace.repo.clone(),
                base_branch: run.workspace.base_branch.clone(),
                branch_prefix: run.workspace.branch_prefix.clone(),
                worktree_mode: Some(run.workspace.worktree_mode.as_str().to_owned()),
                tool: run.workspace.tool.clone(),
                model: run.workspace.model.clone(),
                effort: run.workspace.effort.clone(),
                gate: run.workspace.gate.clone(),
                merge: run.workspace.merge.map(|m| m.as_str().to_owned()),
                auto_pr: run.workspace.auto_pr,
            },
            lifecycle: Some(run.lifecycle.as_str().to_owned()),
            created_at: Some(run.created_at.clone()),
            started_at: run.started_at.clone(),
            hcom_log_cursor: run.hcom_log_cursor.and_then(|c| i64::try_from(c).ok()),
            pr_url: run.pr_url.clone(),
        }
    }

    /// Reconstruct the domain [`Run`].
    pub(crate) fn into_run(self) -> Run {
        Run {
            id: run_key(&self.id),
            title: self.title,
            workspace: Workspace {
                repo: self.workspace.repo,
                base_branch: self.workspace.base_branch,
                branch_prefix: self.workspace.branch_prefix,
                worktree_mode: WorktreeMode::parse(self.workspace.worktree_mode.as_deref()),
                tool: self.workspace.tool,
                model: self.workspace.model,
                effort: self.workspace.effort,
                gate: self.workspace.gate,
                merge: self
                    .workspace
                    .merge
                    .map(|s| MergeMode::parse(Some(s.as_str()))),
                auto_pr: self.workspace.auto_pr,
            },
            lifecycle: Lifecycle::parse(self.lifecycle.as_deref()),
            created_at: self.created_at.unwrap_or_default(),
            started_at: self.started_at,
            hcom_log_cursor: self.hcom_log_cursor.and_then(|c| u64::try_from(c).ok()),
            pr_url: self.pr_url,
        }
    }
}

/// The raw string form of a run id's key (the part after `run:`).
fn run_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
