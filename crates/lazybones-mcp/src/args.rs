//! Shared tool-argument DTOs.
//!
//! MCP tool inputs are JSON objects whose JSON-Schema rmcp derives from
//! `schemars`-derived argument structs (the `#[tool]` macro reads the struct in the
//! method signature). The cross-tool input shapes (ids, pagination, common
//! filters) live here so the `tools::*` modules share one definition rather than
//! redeclaring them per verb.
//!
//! `schemars` is re-exported by `rmcp` (`rmcp::schemars`), so the DTOs derive
//! [`JsonSchema`](rmcp::schemars::JsonSchema) without a separate dep. These are the
//! typed twins of the REST request bodies in
//! [`lazybones-api`'s `dto.rs`](../../../crates/lazybones-api/src/dto.rs); enum
//! fields are carried as strings here and parsed through the store's own
//! string<->enum mappers so the two surfaces accept the same wire shape.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use lazybones_store::{MergeMode, Workspace, WorktreeMode};

/// Arguments for `workflow.create` — the typed twin of the REST `POST /workflows`
/// body ([`CreateWorkflowBody`](../../../crates/lazybones-api/src/dto.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct WorkflowCreateArgs {
    /// Unique workflow id; the call conflicts (409-equivalent) if it is taken.
    pub id: String,
    /// Human title.
    pub title: String,
    /// The repo + inherited git/agent config the workflow's tasks default to.
    pub workspace: WorkspaceArgs,
}

/// The workspace sub-object of [`WorkflowCreateArgs`]. Mirrors the REST
/// [`WorkspaceBody`](../../../crates/lazybones-api/src/dto.rs) field-for-field; the
/// two enum fields (`worktree_mode`, `merge`) are strings here, parsed via the
/// store's own mappers so a client sends the same wire form REST accepts.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct WorkspaceArgs {
    /// Absolute path to the target git repo.
    pub repo: String,
    /// Base branch override; omitted inherits the global `EngineConfig`.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// Branch-prefix override; omitted inherits the global `EngineConfig`.
    #[serde(default)]
    pub branch_prefix: Option<String>,
    /// Default git mode (`new` | `reuse` | `branch` | `shared`); omitted defaults to
    /// `shared`, matching the REST DTO.
    #[serde(default)]
    pub worktree_mode: Option<String>,
    /// Names the shared worktree dir + branch (for `new`/`shared` modes), overriding
    /// the id-derived default. Omitted keeps the default behaviour.
    #[serde(default)]
    pub worktree_name: Option<String>,
    /// Default agent tool for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub tool: Option<String>,
    /// Default model for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub model: Option<String>,
    /// Default effort for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub effort: Option<String>,
    /// Green-build gate commands; omitted/`null` inherits the global gate, an
    /// explicit empty list disables it.
    #[serde(default)]
    pub gate: Option<Vec<String>>,
    /// Merge strategy (`fast-forward` | `merge` | `pr`); omitted/`null` inherits the
    /// global.
    #[serde(default)]
    pub merge: Option<String>,
    /// Open a GitHub PR automatically once every task is done. Omitted/`null` = off.
    #[serde(default)]
    pub auto_pr: Option<bool>,
}

impl WorkspaceArgs {
    /// Build the domain [`Workspace`] the store stores, parsing the enum strings
    /// through the store's own mappers — the same translation the REST route does.
    #[must_use]
    pub fn into_workspace(self) -> Workspace {
        Workspace {
            repo: self.repo,
            base_branch: self.base_branch,
            branch_prefix: self.branch_prefix,
            // Absent → `Shared` (the store enum default + the REST DTO's
            // `#[serde(default)]`), not the parse-fallback `New`.
            worktree_mode: self
                .worktree_mode
                .map_or_else(WorktreeMode::default, |m| WorktreeMode::parse(Some(&m))),
            worktree_name: self.worktree_name,
            tool: self.tool,
            model: self.model,
            effort: self.effort,
            gate: self.gate,
            merge: self.merge.map(|m| MergeMode::parse(Some(&m))),
            auto_pr: self.auto_pr,
        }
    }
}
