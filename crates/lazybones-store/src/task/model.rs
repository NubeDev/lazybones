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

impl WorktreeMode {
    /// The lowercase wire/storage form of this mode.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            WorktreeMode::New => "new",
            WorktreeMode::Reuse => "reuse",
            WorktreeMode::Branch => "branch",
        }
    }

    /// Parse a stored mode string; missing or unknown values fall back to the
    /// default (`New`), so legacy rows and bad data stay isolated-by-default.
    ///
    /// Shared by both the task row and the template row so the string<->enum
    /// mapping lives in exactly one place.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("reuse") => WorktreeMode::Reuse,
            Some("branch") => WorktreeMode::Branch,
            _ => WorktreeMode::New,
        }
    }
}

/// How a *revived* (re-attempted) task should approach its fix.
///
/// A blocked task failed for a reason, so re-running the same prompt unchanged
/// would likely fail the same way. A strategy is the operator's *intent* for the
/// retry, folded into the re-spawn prompt as guidance (see
/// `scheduler::prompt::compose`): aim for the correct long-term fix, or the
/// smallest change that unblocks. The same two strategies drive both a manual
/// strategy-retry and the hands-off auto-retry loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryStrategy {
    /// Fix the root cause properly, even if it touches more code / takes longer.
    LongTerm,
    /// Apply the smallest, fastest change that gets the task green.
    Quick,
}

impl RetryStrategy {
    /// The wire/storage form (`long_term` / `quick`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RetryStrategy::LongTerm => "long_term",
            RetryStrategy::Quick => "quick",
        }
    }

    /// Parse a stored strategy string; unknown/missing → `None` (auto-retry off).
    #[must_use]
    pub fn parse(s: Option<&str>) -> Option<Self> {
        match s {
            Some("long_term") => Some(RetryStrategy::LongTerm),
            Some("quick") => Some(RetryStrategy::Quick),
            _ => None,
        }
    }

    /// The guidance blurb folded into the re-attempt prompt. `reason` is the prior
    /// block reason so the agent knows *what* it is being asked to fix.
    #[must_use]
    pub fn guidance(self, reason: &str) -> String {
        match self {
            RetryStrategy::LongTerm => format!(
                "Your previous attempt was blocked: {reason}. Fix the root cause \
                 properly — choose the correct, maintainable solution even if it \
                 touches more code or takes longer. Do not paper over the failure."
            ),
            RetryStrategy::Quick => format!(
                "Your previous attempt was blocked: {reason}. Apply the smallest, \
                 fastest change that gets this task green. Do not refactor or expand \
                 scope — just unblock it."
            ),
        }
    }
}

/// Last-known state of a task's linked GitHub issue.
///
/// Stored on the task so the reverse poll (issue → task) detects a *change* of
/// state instead of re-acting every tick. Mirrors [`WorktreeMode`]'s
/// `as_str`/`parse` pattern; the wire form is lowercase (`open`/`closed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueSyncState {
    /// The issue was open at the last sync.
    Open,
    /// The issue was closed at the last sync.
    Closed,
}

impl IssueSyncState {
    /// The lowercase wire/storage form (`open` / `closed`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            IssueSyncState::Open => "open",
            IssueSyncState::Closed => "closed",
        }
    }

    /// Parse a stored state string; unknown/missing → `None` (never synced).
    ///
    /// Also accepts the upper-case `OPEN`/`CLOSED` that `gh issue view` emits in
    /// its `state` field, so a live `Issue.state` can be classified directly.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Option<Self> {
        match s.map(str::to_ascii_lowercase).as_deref() {
            Some("open") => Some(IssueSyncState::Open),
            Some("closed") => Some(IssueSyncState::Closed),
            _ => None,
        }
    }
}

/// Extract the issue **number** from a stored issue URL.
///
/// The `gh` issue methods key off the number, not the URL, so the actions and
/// the reverse-sync poll resolve it from the trailing path segment of an
/// `https://github.com/owner/repo/issues/<n>` URL. Returns `None` if the URL
/// has no parseable trailing number.
#[must_use]
pub fn issue_number_from_url(url: &str) -> Option<u64> {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|seg| seg.parse().ok())
}

/// The default cap on hands-off auto-retries before a blocked task waits for a
/// human. Configurable per task via `Task::max_retries`.
pub const DEFAULT_MAX_RETRIES: u32 = 2;

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
    /// Per-task model id forwarded to the agent CLI (e.g. `claude-opus-4-8`);
    /// `None` lets the CLI use its own default. One of the agent's catalog
    /// `models`. See [`AgentCatalog`](crate::AgentCatalog).
    #[serde(default)]
    pub model: Option<String>,
    /// Per-task effort level forwarded to the agent CLI (e.g. `high`); `None`
    /// lets the CLI use its own default. One of the agent's catalog `efforts`.
    #[serde(default)]
    pub effort: Option<String>,
    /// Per-task override of folder-trust auto-seeding (the global
    /// `auto_trust_agent_folder`): pre-seed Claude Code's `hasTrustDialogAccepted`
    /// for this task's worktree before spawning, so a headless `claude` doesn't
    /// freeze on the *"Yes, I trust this folder"* dialog. `None` inherits the
    /// global default (on); `Some(false)` opts this task out.
    #[serde(default)]
    pub auto_trust_agent_folder: Option<bool>,
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
    /// RFC3339 timestamp of when the task first moved to `running` (claimed).
    /// Set once on the first claim and kept across reclaims/revives so it always
    /// reflects when work actually began. `None` until the task starts.
    #[serde(default)]
    pub started_at: Option<String>,
    /// RFC3339 timestamp of when the task reached `done`. `None` until done.
    #[serde(default)]
    pub finished_at: Option<String>,
    /// RFC3339 timestamp of the most recent `blocked` transition (a failure).
    /// Cleared on revive/clean-retry so it reflects the *latest* failure, not a
    /// stale one. `None` while the task has never failed (or was revived).
    #[serde(default)]
    pub failed_at: Option<String>,
    /// FK to the parent workflow [`Run`](crate::Run); `None` for a standalone
    /// task. Distinct from `run` (an event-grouping label): `run_id` is the real
    /// relationship the workflow views key off (SCOPE.md principle 6 — the link
    /// is the truth, the dotted board label is only derived from it).
    #[serde(default)]
    pub run_id: Option<String>,
    /// Provenance: which [`Template`](crate::Template) this task was
    /// instantiated from, if any.
    #[serde(default)]
    pub template_id: Option<String>,
    /// For `worktree_mode = reuse`: the id of the task whose stored `worktree`
    /// this task should reuse (cross-workflow tree sharing).
    #[serde(default)]
    pub reuse_from: Option<String>,
    /// Workflow-only override of the inherited worktree mode. `None` means
    /// "inherit the workspace mode" (the resolver falls back to the run, then
    /// the global default). The non-optional `worktree_mode` above is left as
    /// the standalone-task contract so standalone behaviour is unchanged.
    // TODO(workflow): two worktree-mode fields coexist — `worktree_mode` (the
    // pre-workflow standalone field) and `worktree_mode_override` (the
    // inherit-aware Option the resolver uses when `run_id` is set). A later pass
    // could collapse them once nothing reads the non-optional one directly.
    #[serde(default)]
    pub worktree_mode_override: Option<WorktreeMode>,
    /// Hands-off retry policy: when set, the scheduler re-attempts this task on a
    /// block (with the strategy's guidance) instead of leaving it for a human,
    /// up to `max_retries` times. `None` (the default) keeps the manual-only
    /// behaviour — a block waits for an operator.
    #[serde(default)]
    pub auto_retry: Option<RetryStrategy>,
    /// Cap on hands-off auto-retries before the task stays blocked for a human.
    /// Defaults to [`DEFAULT_MAX_RETRIES`].
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// How many auto-retries have been spent. Reset to 0 by a clean (human)
    /// retry/restart and on `done`; the cap is `retry_count >= max_retries`.
    #[serde(default)]
    pub retry_count: u32,
    /// The linked GitHub issue URL, if any. Covers both "created by us" and
    /// "linked existing" — provenance is irrelevant once linked. `None` =
    /// unlinked. `#[serde(default)]` so existing rows read back unlinked.
    #[serde(default)]
    pub issue_url: Option<String>,
    /// Whether to close the linked issue when this task reaches `done`.
    /// Defaults to `false`; `#[serde(default)]` keeps legacy rows readable.
    #[serde(default)]
    pub issue_close_on_done: bool,
    /// Last-known state of the linked issue, so the reverse poll detects a
    /// *change* instead of re-acting every tick. `None` until first sync.
    #[serde(default)]
    pub issue_synced_state: Option<IssueSyncState>,
}

/// `serde` default for [`Task::max_retries`] (a fn because `serde(default = …)`
/// needs a path, not a literal).
fn default_max_retries() -> u32 {
    DEFAULT_MAX_RETRIES
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
            model: None,
            effort: None,
            auto_trust_agent_folder: None,
            worktree_mode: WorktreeMode::default(),
            session: None,
            worktree: None,
            branch: None,
            commit: None,
            reason: None,
            heartbeat: None,
            started_at: None,
            finished_at: None,
            failed_at: None,
            run_id: None,
            template_id: None,
            reuse_from: None,
            worktree_mode_override: None,
            auto_retry: None,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_count: 0,
            issue_url: None,
            issue_close_on_done: false,
            issue_synced_state: None,
        }
    }
}
