//! The persisted shape of a [`ManagementAgentConfig`] — one fixed row.
//!
//! A single global record lives at `settings:management_agent` (a settings-style
//! row keyed by a constant, mirroring how the agent catalog keys rows by id).
//! Optional columns keep it forward-compatible.

use surrealdb::types::{RecordId, SurrealValue};

use super::model::{ManagementAgentConfig, PermissionProfile, SessionMode};

/// The settings table the management-agent records live in (one per scope).
pub(crate) const SETTINGS_TABLE: &str = "settings";

/// SurrealDB-facing config: the reserved `id` thing plus the config fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct ManagementAgentRow {
    pub(crate) id: RecordId,
    pub(crate) tool: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    /// `"read_only" | "author"`.
    pub(crate) permission_profile: Option<String>,
    /// `"per_conversation" | "per_turn"`.
    pub(crate) session_mode: Option<String>,
    pub(crate) enabled_skills: Option<Vec<String>>,
    pub(crate) permission_flags: Option<Vec<String>>,
    pub(crate) updated_at: Option<String>,
}

impl ManagementAgentRow {
    /// Project a domain [`ManagementAgentConfig`] into its persisted row at
    /// `key` (the scope's storage key).
    pub(crate) fn from_config(key: &str, c: &ManagementAgentConfig) -> Self {
        Self {
            id: RecordId::new(SETTINGS_TABLE, key),
            tool: c.tool.clone(),
            model: c.model.clone(),
            effort: c.effort.clone(),
            permission_profile: Some(c.permission_profile.as_str().to_owned()),
            session_mode: Some(c.session_mode.as_str().to_owned()),
            enabled_skills: Some(c.enabled_skills.clone()),
            permission_flags: Some(c.permission_flags.clone()),
            updated_at: Some(c.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`ManagementAgentConfig`].
    pub(crate) fn into_config(self) -> ManagementAgentConfig {
        ManagementAgentConfig {
            tool: self.tool,
            model: self.model,
            effort: self.effort,
            permission_profile: self
                .permission_profile
                .as_deref()
                .map(PermissionProfile::parse)
                .unwrap_or(PermissionProfile::ReadOnly),
            session_mode: self
                .session_mode
                .as_deref()
                .map(SessionMode::parse)
                .unwrap_or(SessionMode::PerConversation),
            enabled_skills: self.enabled_skills.unwrap_or_default(),
            permission_flags: self.permission_flags.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}
