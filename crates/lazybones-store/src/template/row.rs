//! The persisted shape of a [`Template`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Template`]
//! carries a plain string id. Optional columns keep the row forward-compatible:
//! a field added later reads back as `None` on rows written before it existed.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use crate::task::WorktreeMode;

use super::model::Template;

/// The table templates live in.
pub(crate) const TEMPLATE_TABLE: &str = "template";

/// SurrealDB-facing template: the reserved `id` thing plus the recipe fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct TemplateRow {
    pub(crate) id: RecordId,
    pub(crate) title: String,
    pub(crate) spec_template: String,
    /// `Option` columns so rows written before a field existed read back fine.
    pub(crate) description: Option<String>,
    pub(crate) default_tool: Option<String>,
    /// Worktree mode intent, stored as its lowercase string form; `None` = unset.
    pub(crate) default_worktree_mode: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl TemplateRow {
    /// Project a domain [`Template`] into its persisted row.
    pub(crate) fn from_template(t: &Template) -> Self {
        Self {
            id: RecordId::new(TEMPLATE_TABLE, t.id.as_str()),
            title: t.title.clone(),
            spec_template: t.spec_template.clone(),
            description: Some(t.description.clone()),
            default_tool: t.default_tool.clone(),
            default_worktree_mode: t.default_worktree_mode.map(|m| m.as_str().to_owned()),
            created_at: Some(t.created_at.clone()),
            updated_at: Some(t.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Template`].
    pub(crate) fn into_template(self) -> Template {
        Template {
            id: template_key(&self.id),
            title: self.title,
            spec_template: self.spec_template,
            description: self.description.unwrap_or_default(),
            default_tool: self.default_tool,
            default_worktree_mode: self
                .default_worktree_mode
                .as_deref()
                .map(|s| WorktreeMode::parse(Some(s))),
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a template id's key (the part after `template:`).
fn template_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
