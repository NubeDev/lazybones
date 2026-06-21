//! The persisted shape of a [`Skill`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Skill`] carries
//! a plain string id. Optional columns keep the row forward-compatible: a field
//! added later reads back as `None` on rows written before it existed.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::Skill;

/// The table skills live in.
pub(crate) const SKILL_TABLE: &str = "skill";

/// SurrealDB-facing skill: the reserved `id` thing plus the recipe fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct SkillRow {
    pub(crate) id: RecordId,
    pub(crate) title: String,
    /// `Option` columns so rows written before a field existed read back fine.
    pub(crate) description: Option<String>,
    pub(crate) body: Option<String>,
    /// JSON-serialized [`SkillAction`]; `None` for a plain markdown skill. Stored
    /// as a string so the row stays flat regardless of the action's JSON shape.
    pub(crate) action: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl SkillRow {
    /// Project a domain [`Skill`] into its persisted row.
    pub(crate) fn from_skill(s: &Skill) -> Self {
        Self {
            id: RecordId::new(SKILL_TABLE, s.id.as_str()),
            title: s.title.clone(),
            description: Some(s.description.clone()),
            body: Some(s.body.clone()),
            action: s.action.as_ref().and_then(|a| serde_json::to_string(a).ok()),
            created_at: Some(s.created_at.clone()),
            updated_at: Some(s.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Skill`].
    pub(crate) fn into_skill(self) -> Skill {
        Skill {
            id: skill_key(&self.id),
            title: self.title,
            description: self.description.unwrap_or_default(),
            body: self.body.unwrap_or_default(),
            action: self.action.and_then(|s| serde_json::from_str(&s).ok()),
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a skill id's key (the part after `skill:`).
fn skill_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
