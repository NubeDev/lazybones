//! The durable `Template` document — a reusable, stateless task recipe.
//!
//! A template has no lifecycle, no run, no claim state. It is picked from a list
//! when adding a task to a workflow; [`instantiate`](super::instantiate::instantiate)
//! turns it into a concrete [`Task`](crate::Task). Git mode is normally a property
//! of the *workspace*, not the recipe — `default_worktree_mode` exists only for
//! the rare template intrinsically tied to a mode and should almost always be
//! `None`.
//!
//! Seam for the deferred `Plan` layer: a future `Plan` would be an ordered set of
//! templates instantiated as a whole; this `Template` is the half a Plan reuses.

use serde::{Deserialize, Serialize};

use crate::task::WorktreeMode;

/// A reusable task recipe, unique install-wide by `id` (e.g. `open-pr`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    /// Friendly, unique id (e.g. `code-review`, `open-pr`).
    pub id: String,
    /// Human title.
    pub title: String,
    /// Optional longer description shown in the picker.
    #[serde(default)]
    pub description: String,
    /// Starting spec text for tasks instantiated from this template.
    pub spec_template: String,
    /// Agent tool inherited by the task unless overridden; usually `None`.
    #[serde(default)]
    pub default_tool: Option<String>,
    /// Model inherited by the task unless overridden; `None` lets it inherit the
    /// workflow / global default.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Effort inherited by the task unless overridden; `None` inherits.
    #[serde(default)]
    pub default_effort: Option<String>,
    /// Rarely-set worktree mode intrinsic to the recipe; usually `None` so the
    /// task inherits the workspace mode.
    #[serde(default)]
    pub default_worktree_mode: Option<WorktreeMode>,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Template {
    /// A freshly authored template stamped `created_at == updated_at == now`.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        spec_template: impl Into<String>,
        default_tool: Option<String>,
        default_model: Option<String>,
        default_effort: Option<String>,
        default_worktree_mode: Option<WorktreeMode>,
        now: impl Into<String>,
    ) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            spec_template: spec_template.into(),
            default_tool,
            default_model,
            default_effort,
            default_worktree_mode,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
