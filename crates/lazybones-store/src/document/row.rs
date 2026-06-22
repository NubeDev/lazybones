//! The persisted shape of a [`Document`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`. `kind` is stored lowercase
//! as a string; the optional [`DocRepo`] target rides as a flat sub-object (like
//! the run's [`WorkspaceRow`](crate::run)). `Option` columns keep the row
//! forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::{DocKind, DocRepo, Document};

/// The table documents live in.
pub(crate) const DOCUMENT_TABLE: &str = "document";

/// SurrealDB-facing GitHub-target sub-object.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct DocRepoRow {
    pub(crate) repo: String,
    pub(crate) base_branch: Option<String>,
    pub(crate) branch_prefix: Option<String>,
    pub(crate) output_path: String,
    pub(crate) branch: Option<String>,
    pub(crate) issue_url: Option<String>,
    pub(crate) pr_url: Option<String>,
}

impl DocRepoRow {
    fn from_repo(r: &DocRepo) -> Self {
        Self {
            repo: r.repo.clone(),
            base_branch: r.base_branch.clone(),
            branch_prefix: r.branch_prefix.clone(),
            output_path: r.output_path.clone(),
            branch: r.branch.clone(),
            issue_url: r.issue_url.clone(),
            pr_url: r.pr_url.clone(),
        }
    }

    fn into_repo(self) -> DocRepo {
        DocRepo {
            repo: self.repo,
            base_branch: self.base_branch,
            branch_prefix: self.branch_prefix,
            output_path: self.output_path,
            branch: self.branch,
            issue_url: self.issue_url,
            pr_url: self.pr_url,
        }
    }
}

/// SurrealDB-facing document: the reserved `id` thing plus the document fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct DocumentRow {
    pub(crate) id: RecordId,
    pub(crate) title: String,
    pub(crate) project: Option<String>,
    /// Lowercase `kind` (`document` | `reference`); `None` reads back as
    /// `Document`.
    pub(crate) kind: Option<String>,
    pub(crate) branding_id: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) repo: Option<DocRepoRow>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl DocumentRow {
    /// Project a domain [`Document`] into its persisted row.
    pub(crate) fn from_document(d: &Document) -> Self {
        Self {
            id: RecordId::new(DOCUMENT_TABLE, d.id.as_str()),
            title: d.title.clone(),
            project: d.project.clone(),
            kind: Some(d.kind.as_str().to_owned()),
            branding_id: d.branding_id.clone(),
            body: Some(d.body.clone()),
            repo: d.repo.as_ref().map(DocRepoRow::from_repo),
            created_at: Some(d.created_at.clone()),
            updated_at: Some(d.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Document`].
    pub(crate) fn into_document(self) -> Document {
        Document {
            id: document_key(&self.id),
            title: self.title,
            project: self.project,
            kind: DocKind::parse(self.kind.as_deref()),
            branding_id: self.branding_id,
            body: self.body.unwrap_or_default(),
            repo: self.repo.map(DocRepoRow::into_repo),
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a document id's key (the part after `document:`).
fn document_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
