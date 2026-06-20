//! The durable `Run` document — a Workflow: one concrete, one-off run on a repo.
//!
//! A Run is bound to a **workspace** (the repo + the git config its tasks
//! inherit). Only `lifecycle` (`active | cancelled`) is a stored, human-set
//! field; the user-facing *state* is **derived** from the run's tasks on read
//! (see [`derived_state`](super::derived::derived_state)) so it can never drift
//! from reality (SCOPE.md principle 6 — the DB is truth, not a rollup that lies).
//!
//! Seam for the deferred `Plan` layer: a future `Plan` would be a reusable recipe
//! instantiated into many Runs; this Run is the per-instantiation half a Plan
//! produces. The `paused` lifecycle and a `plan_id`/`plan_version` snapshot are
//! deliberately deferred (see docs/starting-workflows.md).

use serde::{Deserialize, Serialize};

use crate::task::WorktreeMode;

/// The repo + git config a workflow's tasks inherit (per-field, most-specific
/// wins; see the engine's `EffectiveGit` resolver).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    /// Absolute path to the target git repo.
    pub repo: String,
    /// Base branch tasks fork from; `None` inherits the global `EngineConfig`.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// Branch-name prefix; `None` inherits the global `EngineConfig`.
    #[serde(default)]
    pub branch_prefix: Option<String>,
    /// The default git mode for this workflow's tasks.
    #[serde(default)]
    pub worktree_mode: WorktreeMode,
    /// Default agent tool for this workflow's tasks; `None` inherits the global
    /// `EngineConfig`. A task's own `tool` still wins.
    #[serde(default)]
    pub tool: Option<String>,
    /// Default model for this workflow's tasks; `None` inherits the global.
    #[serde(default)]
    pub model: Option<String>,
    /// Default effort for this workflow's tasks; `None` inherits the global.
    #[serde(default)]
    pub effort: Option<String>,
    /// Green-build gate commands for this workflow's tasks. `None`/absent inherits
    /// the global `EngineConfig.gate`; `Some([])` (explicit empty) means **no gate**
    /// — a task lands as soon as its agent reports DONE, running no command.
    #[serde(default)]
    pub gate: Option<Vec<String>>,
}

/// The human-set lifecycle of a Run. Distinct from the derived *state*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    /// The workflow is live; the scheduler may claim its ready tasks.
    #[default]
    Active,
    /// The workflow was cancelled by a human; no more tasks are promoted.
    Cancelled,
}

impl Lifecycle {
    /// The lowercase wire/storage form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Lifecycle::Active => "active",
            Lifecycle::Cancelled => "cancelled",
        }
    }

    /// Parse the stored form; unknown values fall back to `Active`.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("cancelled") => Lifecycle::Cancelled,
            _ => Lifecycle::Active,
        }
    }
}

/// A Workflow (stored in the `run` table), keyed by a friendly id (`workflow-1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Friendly, unique id (e.g. `workflow-1`).
    pub id: String,
    /// Human title.
    pub title: String,
    /// The repo + inherited git config.
    pub workspace: Workspace,
    /// Human-set lifecycle (the only stored state field).
    #[serde(default)]
    pub lifecycle: Lifecycle,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 activation timestamp, set on `start`.
    #[serde(default)]
    pub started_at: Option<String>,
    /// Highest hcom event id ingested into this run's hcom log; `None` = nothing
    /// yet. The persisted ingestion cursor (docs/hcom-logs-scope.md) — the tail
    /// resumes the drain from here on restart, so it holds no in-memory state
    /// (SCOPE.md principle 3). Additive on a `SCHEMALESS` table, so rows written
    /// before it read back as `None` with no migration.
    #[serde(default)]
    pub hcom_log_cursor: Option<u64>,
}

impl Run {
    /// A freshly created, `active` workflow with no tasks yet.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        workspace: Workspace,
        now: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            workspace,
            lifecycle: Lifecycle::Active,
            created_at: now.into(),
            started_at: None,
            hcom_log_cursor: None,
        }
    }
}
