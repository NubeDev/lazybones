//! The persisted shape of a [`Run`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`. The workspace is stored as a
//! flat sub-object; `Option` columns keep the row forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use crate::task::WorktreeMode;

use super::model::{Lifecycle, Run, Workspace};

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
            },
            lifecycle: Some(run.lifecycle.as_str().to_owned()),
            created_at: Some(run.created_at.clone()),
            started_at: run.started_at.clone(),
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
            },
            lifecycle: Lifecycle::parse(self.lifecycle.as_deref()),
            created_at: self.created_at.unwrap_or_default(),
            started_at: self.started_at,
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
