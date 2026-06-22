//! The persisted shape of an [`AgentCatalog`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain type carries a
//! plain string id. Optional columns keep the row forward-compatible: a field
//! added later reads back as `None`/empty on rows written before it existed.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::AgentCatalog;

/// The table agent catalog entries live in.
pub(crate) const AGENT_TABLE: &str = "agent";

/// SurrealDB-facing agent: the reserved `id` thing plus the catalog fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct AgentRow {
    pub(crate) id: RecordId,
    pub(crate) label: String,
    pub(crate) env_var: String,
    /// `Option` columns so rows written before a field existed read back fine.
    pub(crate) login_hint: Option<String>,
    pub(crate) models: Vec<String>,
    pub(crate) default_model: Option<String>,
    pub(crate) efforts: Vec<String>,
    pub(crate) default_effort: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl AgentRow {
    /// Project a domain [`AgentCatalog`] into its persisted row.
    pub(crate) fn from_agent(a: &AgentCatalog) -> Self {
        Self {
            id: RecordId::new(AGENT_TABLE, a.id.as_str()),
            label: a.label.clone(),
            env_var: a.env_var.clone(),
            login_hint: Some(a.login_hint.clone()),
            models: a.models.clone(),
            default_model: a.default_model.clone(),
            efforts: a.efforts.clone(),
            default_effort: a.default_effort.clone(),
            created_at: Some(a.created_at.clone()),
            updated_at: Some(a.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`AgentCatalog`].
    pub(crate) fn into_agent(self) -> AgentCatalog {
        AgentCatalog {
            id: agent_key(&self.id),
            label: self.label,
            env_var: self.env_var,
            login_hint: self.login_hint.unwrap_or_default(),
            models: self.models,
            default_model: self.default_model,
            efforts: self.efforts,
            default_effort: self.default_effort,
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of an agent id's key (the part after `agent:`).
fn agent_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
