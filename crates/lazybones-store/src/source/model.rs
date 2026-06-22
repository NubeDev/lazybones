//! The durable `Source` document — a document's uploads / context material.
//!
//! A source is research material an author adds **behind** a [`Document`] — a
//! link, an uploaded PDF, an image — that is **never rendered into the output**
//! (that is what distinguishes a source from a [`Reference`](crate::DocKind),
//! which *is* merged in). File sources reuse the content-addressed
//! [`BlobStore`](crate::BlobStore) + sha256 dedup via an [`Asset`](crate::Asset)
//! (`asset_id`). On PDF upload, plain text is extracted into `extracted_text`
//! (see [`extract_pdf_text`](super::extract_pdf_text)) — it powers preview/keyword
//! search now and is the exact substrate the later RAG phase chunks + embeds.
//!
//! The source↔document link also rides the generic
//! [`attachment`](crate::attachment) seam (`thing_kind="source"`); the `document`
//! field here is the direct FK the per-document listing queries.

use serde::{Deserialize, Serialize};

/// What a source *is*: an external link, or an uploaded file. Serialized lowercase
/// (`link` | `file`), matching the [`MergeMode`](crate::MergeMode) convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// An external URL the author references.
    #[default]
    Link,
    /// An uploaded file, stored via the asset server (`asset_id`).
    File,
}

impl SourceKind {
    /// The lowercase wire/storage form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Link => "link",
            SourceKind::File => "file",
        }
    }

    /// Parse the stored form; unknown/missing values fall back to `Link`.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("file") => SourceKind::File,
            _ => SourceKind::Link,
        }
    }
}

/// A document's upload / context item, unique install-wide by `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Friendly, unique id (typically minted by the caller, e.g. a ULID).
    pub id: String,
    /// The id of the [`Document`](crate::Document) this source sits behind.
    pub document: String,
    /// Optional project scope; always `None` today (projects land later).
    #[serde(default)]
    pub project: Option<String>,
    /// Whether this source is a link or an uploaded file.
    #[serde(default)]
    pub kind: SourceKind,
    /// The URL, for a `Link` source.
    #[serde(default)]
    pub url: Option<String>,
    /// The [`Asset`](crate::Asset) id holding the bytes, for a `File` source.
    #[serde(default)]
    pub asset_id: Option<String>,
    /// Human title / label.
    #[serde(default)]
    pub title: String,
    /// The MIME content type (e.g. `application/pdf`), for a file source.
    #[serde(default)]
    pub content_type: String,
    /// Extracted plain text (from a PDF, today); `None` until extracted.
    #[serde(default)]
    pub extracted_text: Option<String>,
    /// RFC3339 creation timestamp.
    pub created_at: String,
}

impl Source {
    /// A `Link` source pointing at `url`, stamped `created_at == now`.
    #[must_use]
    pub fn link(
        id: impl Into<String>,
        document: impl Into<String>,
        url: impl Into<String>,
        title: impl Into<String>,
        now: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            document: document.into(),
            project: None,
            kind: SourceKind::Link,
            url: Some(url.into()),
            asset_id: None,
            title: title.into(),
            content_type: String::new(),
            extracted_text: None,
            created_at: now.into(),
        }
    }

    /// A `File` source backed by `asset_id`, stamped `created_at == now`.
    #[must_use]
    pub fn file(
        id: impl Into<String>,
        document: impl Into<String>,
        asset_id: impl Into<String>,
        title: impl Into<String>,
        content_type: impl Into<String>,
        now: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            document: document.into(),
            project: None,
            kind: SourceKind::File,
            url: None,
            asset_id: Some(asset_id.into()),
            title: title.into(),
            content_type: content_type.into(),
            extracted_text: None,
            created_at: now.into(),
        }
    }

    /// Attach extracted plain text (builder style).
    #[must_use]
    pub fn with_extracted_text(mut self, text: impl Into<String>) -> Self {
        self.extracted_text = Some(text.into());
        self
    }
}
