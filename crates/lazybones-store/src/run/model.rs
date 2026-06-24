//! The durable `Run` document — a Workflow: one concrete, one-off run on a repo.
//!
//! A Run is bound to a **workspace** (the repo + the git config its tasks
//! inherit). Only `lifecycle` (`active | stopped`) is a stored, human-set
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

/// How a green task branch lands back on the base branch — the storable, wire
/// form of the engine's merge strategy.
///
/// Mirrors `WorktreeMode`: the store owns the string<->enum mapping so a workspace
/// can pin a strategy, and the engine maps it onto its own `config::MergeMode` in
/// the `EffectiveGit` resolver. A workflow that omits it inherits the global
/// `EngineConfig.merge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeMode {
    /// Fast-forward `base` to the task branch; refuse if base moved.
    #[default]
    FastForward,
    /// Create a merge commit of the task branch into `base`.
    Merge,
    /// Push the branch only; open a PR out of band.
    Pr,
}

impl MergeMode {
    /// The wire/storage form (`fast-forward` | `merge` | `pr`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MergeMode::FastForward => "fast-forward",
            MergeMode::Merge => "merge",
            MergeMode::Pr => "pr",
        }
    }

    /// Parse a stored mode string; missing or unknown values fall back to the
    /// default (`FastForward`) — fail safe, matching the engine config parser.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("merge") => MergeMode::Merge,
            Some("pr") => MergeMode::Pr,
            _ => MergeMode::FastForward,
        }
    }
}

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
    /// Names the worktree dir + branch for `New`/`Shared` modes, overriding the
    /// id-derived default (`run_id` for Shared, `task.id` for New). `None` keeps
    /// today's behaviour. Two workflows that set the **same** name land in the
    /// **same** tree on the **same** branch — the supported way for several
    /// workflows to build in one shared worktree. Pick an existing worktree's
    /// name to attach to it, or a fresh name to create one. Ignored by
    /// `Reuse`/`Branch` modes (they don't create an id-keyed tree).
    #[serde(default)]
    pub worktree_name: Option<String>,
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
    /// How this workflow's green branches land back on base. `None`/absent inherits
    /// the global `EngineConfig.merge`, so a repo wanting strict linear history can
    /// pin `fast-forward` while others use `merge`.
    #[serde(default)]
    pub merge: Option<MergeMode>,
    /// When `true`, the engine opens a GitHub PR for this workflow's branch once
    /// every task is `done`: it spawns the workflow's configured agent to write a
    /// summary, then `gh pr create`s it (idempotent — at most one PR per run, the
    /// url recorded on the [`Run`]). `None`/`false` = off. Most useful with `Shared`
    /// mode (one branch → one PR); a no-op for auto-merge modes that have no branch
    /// left to PR. The PR opens against `base_branch`.
    #[serde(default)]
    pub auto_pr: Option<bool>,
}

/// The human-set lifecycle of a Run. Distinct from the derived *state*.
///
/// Only `done` (derived) and a hard `delete` are truly terminal — every
/// lifecycle here is reversible, so the UI never claims a run is finished when it
/// is not. A human pauses a run into `Stopped` (the scheduler then promotes/claims
/// nothing) and flips it back to `Active` with resume. The old terminal
/// `Cancelled` tombstone is gone: `delete` is the archive path, and any legacy
/// `cancelled` row reads back as `Stopped` (resumable) — no migration needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lifecycle {
    /// The workflow is live; the scheduler may promote and claim its tasks.
    #[default]
    Active,
    /// The workflow was paused by a human: the scheduler promotes/claims nothing
    /// until it is resumed. Fully reversible (resume → `Active`), never terminal.
    Stopped,
}

impl Lifecycle {
    /// The lowercase wire/storage form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Lifecycle::Active => "active",
            Lifecycle::Stopped => "stopped",
        }
    }

    /// Parse the stored form; unknown values fall back to `Active`. The legacy
    /// `cancelled` tombstone maps to `Stopped` so old rows become resumable
    /// without a migration.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("stopped" | "cancelled") => Lifecycle::Stopped,
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
    /// The URL of the PR the engine auto-opened for this run, once it has. `None`
    /// until the auto-PR flow succeeds; set once and used as the idempotency guard
    /// so a completed run opens **at most one** PR (the tick re-checks every pass).
    /// Additive on the `SCHEMALESS` table — older rows read back `None`.
    #[serde(default)]
    pub pr_url: Option<String>,
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
            pr_url: None,
        }
    }
}
