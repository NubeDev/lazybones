//! The durable `Document` document — an authored, branded *book*.
//!
//! A document is a container: its content lives in its ordered
//! [`page`](crate::page) rows (the book's pages), assembled in `position` order at
//! render time. The document itself carries an optional `branding_id` (resolved
//! against the standalone [`branding`](crate::branding) catalogue at render time)
//! and an optional [`DocRepo`] GitHub target. Its [`kind`](DocKind) distinguishes
//! a normal `Document` from a `Reference` — a reusable document (e.g. Terms &
//! Conditions) merged into other documents' rendered output via the generic
//! [`attachment`](crate::attachment) seam (`thing_kind="reference"`).
//!
//! *References* (merged into the rendered output) are distinct from
//! [*sources*](crate::source) (research material behind the doc that never
//! renders) — both ride the attachment seam with different `thing_kind`s.

use serde::{Deserialize, Serialize};

/// What a document *is*: a normal authored document, or a reusable reference page
/// merged into others' output. Serialized lowercase (`document` | `reference`),
/// matching the [`MergeMode`](crate::MergeMode) enum convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    /// A normal authored document.
    #[default]
    Document,
    /// A reusable page (e.g. T&C) merged into other documents' rendered output.
    Reference,
}

impl DocKind {
    /// The lowercase wire/storage form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DocKind::Document => "document",
            DocKind::Reference => "reference",
        }
    }

    /// Parse the stored form; unknown/missing values fall back to `Document`.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("reference") => DocKind::Reference,
            _ => DocKind::Document,
        }
    }
}

/// The optional GitHub target + linkage for a document, mirroring the workflow
/// [`Workspace`](crate::Workspace) + the task issue-linkage shape. The
/// `branch`/`*_url` fields are filled in as GitHub actions run (like the task's
/// `set_issue_link`); they start empty when the repo target is first set.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DocRepo {
    /// Absolute path to the local git checkout the rendered doc is committed to.
    pub repo: String,
    /// Base branch to fork from; `None` inherits the global/project default.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// Branch-name prefix; `None` inherits the global/project default.
    #[serde(default)]
    pub branch_prefix: Option<String>,
    /// Where in the repo the rendered doc is written, e.g. `docs/<id>.md`.
    pub output_path: String,
    /// The branch created for this document's changes; filled by `gh/branch`.
    #[serde(default)]
    pub branch: Option<String>,
    /// The issue opened from this document; filled by `gh/issue`.
    #[serde(default)]
    pub issue_url: Option<String>,
    /// The PR opened for this document; filled by `gh/pr`.
    #[serde(default)]
    pub pr_url: Option<String>,
}

/// An authored markdown document, unique install-wide by `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Friendly, unique id.
    pub id: String,
    /// Human title.
    pub title: String,
    /// Optional project scope; always `None` today (projects land later).
    #[serde(default)]
    pub project: Option<String>,
    /// Whether this is a normal document or a reusable reference page.
    #[serde(default)]
    pub kind: DocKind,
    /// The brand profile to render with; resolved against the standalone
    /// [`branding`](crate::branding) catalogue. `None` falls back to the default.
    #[serde(default)]
    pub branding_id: Option<String>,
    /// The optional GitHub publishing target + linkage.
    #[serde(default)]
    pub repo: Option<DocRepo>,
    /// Layout: print a page number on every page. Persisted so the toggle sticks
    /// across reloads; the render/export query can still override it for a live
    /// preview. Defaults to `false`.
    #[serde(default)]
    pub page_numbers: bool,
    /// Layout: prepend a table-of-contents index page. Persisted like
    /// [`page_numbers`](Document::page_numbers). Defaults to `false`.
    #[serde(default)]
    pub index: bool,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Document {
    /// A freshly authored document stamped `created_at == updated_at == now`, of
    /// the given `kind`, with no branding/repo and no pages yet.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        kind: DocKind,
        now: impl Into<String>,
    ) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            title: title.into(),
            project: None,
            kind,
            branding_id: None,
            repo: None,
            page_numbers: false,
            index: false,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Set the GitHub publishing target (builder style).
    #[must_use]
    pub fn with_repo(mut self, repo: DocRepo) -> Self {
        self.repo = Some(repo);
        self
    }

    /// Set the persisted layout toggles (builder style).
    #[must_use]
    pub fn with_layout(mut self, page_numbers: bool, index: bool) -> Self {
        self.page_numbers = page_numbers;
        self.index = index;
        self
    }
}
