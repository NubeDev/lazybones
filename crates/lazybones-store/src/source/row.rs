//! The persisted shape of a [`Source`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`. `kind` is stored lowercase
//! as a string; `document` is the FK the per-document listing queries. `Option`
//! columns keep the row forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::{Source, SourceKind};

/// The table sources live in.
pub(crate) const SOURCE_TABLE: &str = "source";

/// SurrealDB-facing source: the reserved `id` thing plus the source fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct SourceRow {
    pub(crate) id: RecordId,
    pub(crate) document: String,
    pub(crate) project: Option<String>,
    /// Lowercase `kind` (`link` | `file`); `None` reads back as `Link`.
    pub(crate) kind: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) asset_id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) content_type: Option<String>,
    pub(crate) extracted_text: Option<String>,
    pub(crate) created_at: Option<String>,
}

impl SourceRow {
    /// Project a domain [`Source`] into its persisted row.
    pub(crate) fn from_source(s: &Source) -> Self {
        Self {
            id: RecordId::new(SOURCE_TABLE, s.id.as_str()),
            document: s.document.clone(),
            project: s.project.clone(),
            kind: Some(s.kind.as_str().to_owned()),
            url: s.url.clone(),
            asset_id: s.asset_id.clone(),
            title: Some(s.title.clone()),
            content_type: Some(s.content_type.clone()),
            extracted_text: s.extracted_text.clone(),
            created_at: Some(s.created_at.clone()),
        }
    }

    /// Reconstruct the domain [`Source`].
    pub(crate) fn into_source(self) -> Source {
        Source {
            id: source_key(&self.id),
            document: self.document,
            project: self.project,
            kind: SourceKind::parse(self.kind.as_deref()),
            url: self.url,
            asset_id: self.asset_id,
            title: self.title.unwrap_or_default(),
            content_type: self.content_type.unwrap_or_default(),
            extracted_text: self.extracted_text,
            created_at: self.created_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a source id's key (the part after `source:`).
fn source_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
