//! The persisted shape of a [`Team`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Team`] carries a
//! plain string id (decisions §3). Optional columns keep the row
//! forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::Team;

/// The table teams live in.
pub(crate) const TEAM_TABLE: &str = "team";

/// SurrealDB-facing team: the reserved `id` thing plus the identity fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct TeamRow {
    pub(crate) id: RecordId,
    pub(crate) title: String,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl TeamRow {
    /// Project a domain [`Team`] into its persisted row.
    pub(crate) fn from_team(t: &Team) -> Self {
        Self {
            id: RecordId::new(TEAM_TABLE, t.id.as_str()),
            title: t.title.clone(),
            created_at: Some(t.created_at.clone()),
            updated_at: Some(t.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Team`].
    pub(crate) fn into_team(self) -> Team {
        Team {
            id: team_key(&self.id),
            title: self.title,
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a team id's key (the part after `team:`).
fn team_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
