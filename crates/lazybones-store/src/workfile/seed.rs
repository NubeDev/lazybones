//! The seed task shape shared by the REST sync route and the CLI boot import.
//!
//! A seed is a workfile task with its spec already resolved to text and no
//! lifecycle/claim fields — the pre-image of a [`Task`](crate::Task) before the
//! store assigns it a `pending` status.

use serde::{Deserialize, Serialize};

/// One task as it arrives for import.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedTask {
    /// Friendly concept id.
    pub id: String,
    /// Human title.
    pub title: String,
    /// Full spec text (already resolved from any `tasks/<id>.md` path).
    pub spec: String,
    /// Dependency ids.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Ownership globs.
    #[serde(default)]
    pub owns: Vec<String>,
    /// Optional per-task agent tool.
    #[serde(default)]
    pub tool: Option<String>,
    /// For `worktree_mode = reuse`: the id of the task whose worktree this task
    /// continues. Implies a `depends_on` edge on that task (the reuse source
    /// must finish first), wired automatically on sync.
    #[serde(default)]
    pub reuse_from: Option<String>,
}
