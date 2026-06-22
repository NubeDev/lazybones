//! The durable `AgentCatalog` document — a CRUD-able agent CLI definition.
//!
//! Today's static catalog (the env var + login hint per tool) lived in code; this
//! moves it into the store so an operator can add/edit/remove agents and, more to
//! the point, declare the **models** and **effort levels** each one offers. The
//! add-task UI reads these to populate its agent / model / effort pickers, instead
//! of hardcoding lists that go stale as providers ship new models.
//!
//! The `id` matches the hcom tool key (`hcom status --json`) — hcom still owns
//! *which* CLIs are installed; this row layers the credential metadata and the
//! model/effort menus on top. Seeded with 2026 defaults at boot (see
//! `seed_default_agents`); the seed never clobbers an operator's edits.

use serde::{Deserialize, Serialize};

/// One agent CLI lazybones can run, with the models and effort levels it offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCatalog {
    /// The tool id — matches the hcom `tools` key (e.g. `claude`, `codex`).
    pub id: String,
    /// Human label for the UI (e.g. `Claude Code`).
    pub label: String,
    /// The env var the CLI reads its credential from (e.g. `ANTHROPIC_API_KEY`).
    pub env_var: String,
    /// How to obtain a credential / log in (shown as a hint in the UI).
    #[serde(default)]
    pub login_hint: String,
    /// Selectable model ids, most-preferred first (e.g. `claude-opus-4-8`).
    /// Empty means "the CLI's own default" — no model picker shown.
    #[serde(default)]
    pub models: Vec<String>,
    /// The model used when a task names none; should be one of `models`.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Selectable effort levels (e.g. `low`, `medium`, `high`, `max`). Empty for
    /// agents with no effort knob — no effort picker shown.
    #[serde(default)]
    pub efforts: Vec<String>,
    /// The effort used when a task names none; should be one of `efforts`.
    #[serde(default)]
    pub default_effort: Option<String>,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl AgentCatalog {
    /// A freshly authored agent stamped `created_at == updated_at == now`.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        env_var: impl Into<String>,
        login_hint: impl Into<String>,
        models: Vec<String>,
        default_model: Option<String>,
        efforts: Vec<String>,
        default_effort: Option<String>,
        now: impl Into<String>,
    ) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            label: label.into(),
            env_var: env_var.into(),
            login_hint: login_hint.into(),
            models,
            default_model,
            efforts,
            default_effort,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// The editable (operator-authored) fields of an agent catalog entry.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentCatalogEdit {
    /// New human label.
    pub label: String,
    /// New credential env var.
    pub env_var: String,
    /// New login hint.
    pub login_hint: String,
    /// New model menu.
    pub models: Vec<String>,
    /// New default model.
    pub default_model: Option<String>,
    /// New effort menu.
    pub efforts: Vec<String>,
    /// New default effort.
    pub default_effort: Option<String>,
}
