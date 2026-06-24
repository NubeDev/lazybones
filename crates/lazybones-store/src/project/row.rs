//! The persisted shape of a [`Project`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Project`] carries
//! a plain string id (decisions §3). `status` and `team` are declared columns with
//! `TYPE string` (see `init_schema`), so they are always written as plain strings —
//! a teamless project stores `team = ""`, read back as `None`. The remaining
//! columns are `Option` for forward-compatibility.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::{Project, ProjectStatus};

/// The table projects live in.
pub(crate) const PROJECT_TABLE: &str = "project";

/// SurrealDB-facing project: the reserved `id` thing plus the project fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct ProjectRow {
    pub(crate) id: RecordId,
    pub(crate) title: String,
    /// Declared `TYPE string` column — always written (`active` | `archived`).
    pub(crate) status: String,
    /// Declared `TYPE string` column — denormalized owning team, `""` when none.
    pub(crate) team: String,
    pub(crate) repos: Option<Vec<String>>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl ProjectRow {
    /// Project a domain [`Project`] into its persisted row.
    pub(crate) fn from_project(p: &Project) -> Self {
        Self {
            id: RecordId::new(PROJECT_TABLE, p.id.as_str()),
            title: p.title.clone(),
            status: p.status.as_str().to_owned(),
            team: p.team.clone().unwrap_or_default(),
            repos: Some(p.repos.clone()),
            created_at: Some(p.created_at.clone()),
            updated_at: Some(p.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Project`].
    pub(crate) fn into_project(self) -> Project {
        Project {
            id: project_key(&self.id),
            title: self.title,
            status: ProjectStatus::parse(Some(self.status.as_str())),
            team: Some(self.team).filter(|t| !t.is_empty()),
            repos: self.repos.unwrap_or_default(),
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a project id's key (the part after `project:`).
fn project_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
