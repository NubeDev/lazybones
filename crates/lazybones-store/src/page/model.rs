//! The durable `Page` document — one ordered section of a [`Document`].
//!
//! A [`Document`](crate::Document) is a *book*: its rendered output is its pages
//! assembled in `position` order (each page becomes a page-break boundary in the
//! exported PDF). A page carries its own markdown `body`; the document-level
//! `branding`/`repo`/sources describe the whole book and live on the
//! [`Document`](crate::Document).
//!
//! Order is a fractional [`position`](Page::position): inserting between two pages
//! takes the midpoint of their positions and appending takes `last + 1.0`, so a
//! reorder or insert rewrites **one** row rather than renumbering the whole book.
//! See [`position_between`] and [`append_position`].

use serde::{Deserialize, Serialize};

/// The gap between freshly appended pages. Keeping it `> 0` leaves room to insert
/// between any two neighbours via [`position_between`] without renumbering.
const POSITION_STEP: f64 = 1.0;

/// The default for [`Page::page_break`]: a page renders on its own unless the
/// author explicitly drops it when empty. Used as the `serde` default so rows
/// written before this field existed deserialize as "always render".
fn default_page_break() -> bool {
    true
}

/// One ordered section of a [`Document`](crate::Document), unique install-wide by
/// `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    /// Friendly, unique id (typically minted by the caller, e.g. a ULID).
    pub id: String,
    /// The id of the [`Document`](crate::Document) (book) this page belongs to.
    pub document: String,
    /// Optional project scope; always `None` today (projects land later).
    #[serde(default)]
    pub project: Option<String>,
    /// Human title / running header for the page.
    #[serde(default)]
    pub title: String,
    /// The page body (markdown).
    #[serde(default)]
    pub body: String,
    /// Fractional sort key within the document; pages render in ascending order.
    #[serde(default)]
    pub position: f64,
    /// Whether this page is always emitted as its own sheet/PDF page — **even when
    /// its body is empty**. The author's "page break" toggle: a blank page with
    /// this set still produces a real page (a deliberate spacer / section break),
    /// whereas an empty page with it unset is dropped from the render. Defaults to
    /// `true` so a normal page always renders.
    #[serde(default = "default_page_break")]
    pub page_break: bool,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Page {
    /// A freshly authored page stamped `created_at == updated_at == now`.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        document: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
        position: f64,
        now: impl Into<String>,
    ) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            document: document.into(),
            project: None,
            title: title.into(),
            body: body.into(),
            position,
            page_break: true,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Set whether this page renders even when empty (builder style).
    #[must_use]
    pub fn with_page_break(mut self, page_break: bool) -> Self {
        self.page_break = page_break;
        self
    }

    /// Whether this page contributes a sheet/PDF page to the render: any page with
    /// content, plus an explicitly kept empty page (its [`page_break`](Page::page_break)
    /// flag). An empty page with the flag off is dropped.
    #[must_use]
    pub fn renders(&self) -> bool {
        self.page_break || !self.body.trim().is_empty()
    }
}

/// The position for a page appended after the current `last` page (or the first
/// page when the book is empty).
#[must_use]
pub fn append_position(last: Option<f64>) -> f64 {
    match last {
        Some(p) => p + POSITION_STEP,
        None => POSITION_STEP,
    }
}

/// A fractional position that sorts strictly between `before` and `after`.
///
/// - both bounds: their midpoint;
/// - only `before` (inserting at the end): `before + step`;
/// - only `after` (inserting at the front): `after - step`;
/// - neither (empty book): the first step.
#[must_use]
pub fn position_between(before: Option<f64>, after: Option<f64>) -> f64 {
    match (before, after) {
        (Some(b), Some(a)) => (b + a) / 2.0,
        (Some(b), None) => b + POSITION_STEP,
        (None, Some(a)) => a - POSITION_STEP,
        (None, None) => POSITION_STEP,
    }
}
