//! The persisted shape of a [`Page`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`. `document` is the FK the
//! per-document listing queries; `position` is the fractional sort key.
//! `Option` columns keep the row forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::Page;

/// The table pages live in.
pub(crate) const PAGE_TABLE: &str = "page";

/// SurrealDB-facing page: the reserved `id` thing plus the page fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct PageRow {
    pub(crate) id: RecordId,
    pub(crate) document: String,
    pub(crate) project: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) position: Option<f64>,
    pub(crate) page_break: Option<bool>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl PageRow {
    /// Project a domain [`Page`] into its persisted row.
    pub(crate) fn from_page(p: &Page) -> Self {
        Self {
            id: RecordId::new(PAGE_TABLE, p.id.as_str()),
            document: p.document.clone(),
            project: p.project.clone(),
            title: Some(p.title.clone()),
            body: Some(p.body.clone()),
            position: Some(p.position),
            page_break: Some(p.page_break),
            created_at: Some(p.created_at.clone()),
            updated_at: Some(p.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Page`].
    pub(crate) fn into_page(self) -> Page {
        Page {
            id: page_key(&self.id),
            document: self.document,
            project: self.project,
            title: self.title.unwrap_or_default(),
            body: self.body.unwrap_or_default(),
            position: self.position.unwrap_or_default(),
            // Rows written before this column default to "always render".
            page_break: self.page_break.unwrap_or(true),
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a page id's key (the part after `page:`).
fn page_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
