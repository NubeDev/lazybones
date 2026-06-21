//! The Lazybones-Agent configuration model — a single global record.
//!
//! One management agent per install (MVP). It is a conversational operator aide
//! that authors workflows/tasks/templates/skills and explains run state, acting
//! only through the same REST surface a human uses
//! (`docs/agent/lazybones-agent-scope.md` §5). The record selects which tool the
//! agent runs as, its model/effort, the permission profile that bounds its
//! scoped token, and which skills it may use as operating runbooks.

use serde::{Deserialize, Serialize};

/// How much of the operator's REST surface the agent's scoped token may exercise.
///
/// A strict subset of the operator's capabilities; `Claim`/`Secret` are never
/// granted, and lifecycle (start/stop/retry/delete) is **not** reachable in
/// Phase 1 — the agent authors, the human starts (scope §9, §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionProfile {
    /// GET-only: explain state, draft specs in chat, never mutate.
    ReadOnly,
    /// + create/edit workflows, tasks, templates, skills, attachments (default).
    Author,
    /// Author, plus the ability to *propose* lifecycle actions
    /// (start/stop/retry/cancel/delete). Each is still confirmed in the UI and
    /// issued under the operator's token, never the agent's (scope §10.2).
    AuthorAndManage,
}

impl PermissionProfile {
    /// The lowercase wire/storage form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionProfile::ReadOnly => "read_only",
            PermissionProfile::Author => "author",
            PermissionProfile::AuthorAndManage => "author_and_manage",
        }
    }

    /// Parse a stored profile string; anything unknown falls back to the safe
    /// `ReadOnly` (never silently grant `Author`/manage).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "author" => PermissionProfile::Author,
            "author_and_manage" => PermissionProfile::AuthorAndManage,
            _ => PermissionProfile::ReadOnly,
        }
    }

    /// Whether this profile carries the `Author` capability (author or manage).
    #[must_use]
    pub fn is_author(self) -> bool {
        matches!(
            self,
            PermissionProfile::Author | PermissionProfile::AuthorAndManage
        )
    }

    /// Whether this profile may *propose* gated lifecycle actions.
    #[must_use]
    pub fn can_manage(self) -> bool {
        matches!(self, PermissionProfile::AuthorAndManage)
    }
}

/// Whether an hcom session is kept across a conversation's turns or spun fresh
/// each turn (scope §11 open question 7 — surfaced to the operator).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// One hcom session per conversation, resumed each turn — keeps context,
    /// killed on idle/end via `kill_tag`.
    PerConversation,
    /// A fresh hcom session each turn, with history replayed into the prompt.
    PerTurn,
}

impl SessionMode {
    /// The lowercase wire/storage form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SessionMode::PerConversation => "per_conversation",
            SessionMode::PerTurn => "per_turn",
        }
    }

    /// Parse a stored mode string; default to `PerConversation`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "per_turn" => SessionMode::PerTurn,
            _ => SessionMode::PerConversation,
        }
    }
}

/// Which scope a Lazybones-Agent configuration applies to (scope §11 Q1). The
/// `Global` record is the install-wide default; a `Workflow` record overrides it
/// for one workflow. A turn resolves `workflow-override ?? global`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagementAgentScope {
    /// The install-wide default configuration.
    Global,
    /// An override for one workflow id.
    Workflow(String),
}

impl ManagementAgentScope {
    /// The storage key suffix for this scope: `"management_agent"` for global,
    /// `"management_agent:workflow:<id>"` for a workflow override.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            ManagementAgentScope::Global => "management_agent".to_owned(),
            ManagementAgentScope::Workflow(id) => format!("management_agent:workflow:{id}"),
        }
    }
}

/// The single global Lazybones-Agent configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagementAgentConfig {
    /// FK into the agent catalog, e.g. `"claude"`.
    pub tool: String,
    /// Validated ⊆ `catalog.models` for `tool`; `None` lets the CLI default.
    pub model: Option<String>,
    /// Validated ⊆ `catalog.efforts` for `tool`; `None` lets the CLI default.
    pub effort: Option<String>,
    /// The permission profile bounding the agent's scoped token.
    pub permission_profile: PermissionProfile,
    /// hcom session lifecycle per conversation.
    pub session_mode: SessionMode,
    /// Skill ids the agent may use as operating runbooks (their bodies are
    /// folded into the system prompt).
    pub enabled_skills: Vec<String>,
    /// Extra CLI flags for the tool process (mirrors `EngineConfig`).
    pub permission_flags: Vec<String>,
    /// RFC3339 timestamp of the last write.
    pub updated_at: String,
}

impl Default for ManagementAgentConfig {
    /// A usable default the API returns when nothing has been configured yet:
    /// `claude`, `Author`, per-conversation sessions, no skills enabled.
    ///
    /// `permission_flags` is **empty** by design — unlike task agents, the
    /// management agent runs in a scratch dir bootstrapped with a Claude
    /// allow-list (`.claude/settings.json`), so it does NOT need (and must not
    /// use) `--dangerously-skip-permissions`. That flag triggers Claude Code's
    /// bypass-permissions consent screen, which a headless agent cannot answer
    /// and which the allow-list already makes unnecessary.
    fn default() -> Self {
        Self {
            tool: "claude".to_owned(),
            model: None,
            effort: None,
            permission_profile: PermissionProfile::Author,
            session_mode: SessionMode::PerConversation,
            enabled_skills: Vec::new(),
            permission_flags: Vec::new(),
            updated_at: String::new(),
        }
    }
}
