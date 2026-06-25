//! The pure inputs `lazybones-render` renders from.
//!
//! The crate has **no store dependency**: the API assembles a document (merging
//! its reusable `reference` pages into one markdown blob and fetching the logo +
//! inline image bytes from the `BlobStore`) and hands the result here as plain
//! values. That keeps rendering pure and unit-testable. The store's `Branding`
//! type is intentionally mirrored as a small local [`Brand`] so this crate never
//! pulls in `lazybones-store`.

/// A fully-assembled document ready to render: its title, its ordered **pages**
/// (each page's resolved markdown — the document's own pages followed by each
/// merged reference page, in attach order), the resolved brand profile, and any
/// binary images (logo + inline) already fetched from the blob store.
///
/// Pages are kept as a list rather than one blob so the renderer can put a real
/// page break between them: each entry becomes its own PDF page (and its own card
/// in the HTML preview).
#[derive(Debug, Clone, Default)]
pub struct Assembled {
    /// The document title (rendered as the cover heading).
    pub title: String,
    /// The document's pages, in render order. Each entry is one page's markdown
    /// and becomes one PDF page (page-broken from its neighbours).
    pub pages: Vec<String>,
    /// The resolved brand profile (colors, fonts, header/footer). `Default` is a
    /// neutral, unbranded look.
    pub brand: Brand,
    /// The brand logo, already fetched from the blob store, if the brand sets one.
    pub logo: Option<ImageAsset>,
    /// Inline images referenced by the markdown (`![alt](src)`), each already
    /// fetched from the blob store and keyed by the markdown `src` it resolves.
    pub images: Vec<ImageAsset>,
    /// The title of each entry in [`pages`](Assembled::pages), positionally
    /// aligned, used to build the index. May be empty (the index then falls back
    /// to generic "Page N" labels) so callers that don't track titles still work.
    pub page_titles: Vec<String>,
    /// Layout options the author toggles (page numbers, table-of-contents index).
    pub options: RenderOptions,
}

/// Author-facing layout toggles that affect both the PDF export and the HTML
/// preview. All default to off so an unconfigured document renders as before.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderOptions {
    /// Print a page number in the footer area of every page.
    pub page_numbers: bool,
    /// Prepend a table-of-contents index page listing each page's title.
    pub index: bool,
}

impl Assembled {
    /// A bare single-page document with just a title and markdown and the default
    /// (unbranded) look. Builder-style setters layer brand/logo/images on top.
    /// Use [`with_pages`](Assembled::with_pages) for a multi-page book.
    #[must_use]
    pub fn new(title: impl Into<String>, markdown: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            pages: vec![markdown.into()],
            ..Self::default()
        }
    }

    /// A document assembled from an explicit list of page markdowns (the API's
    /// path: a document's own pages followed by merged-reference pages).
    #[must_use]
    pub fn with_pages(title: impl Into<String>, pages: Vec<String>) -> Self {
        Self {
            title: title.into(),
            pages,
            ..Self::default()
        }
    }

    /// The pages joined into one markdown blob (blank-line separated) — for
    /// consumers that don't care about page boundaries (image discovery, the
    /// committed `.md` file).
    #[must_use]
    pub fn combined_markdown(&self) -> String {
        self.pages.join("\n\n")
    }

    /// Set the brand profile (builder style).
    #[must_use]
    pub fn with_brand(mut self, brand: Brand) -> Self {
        self.brand = brand;
        self
    }

    /// Set the logo bytes (builder style).
    #[must_use]
    pub fn with_logo(mut self, logo: ImageAsset) -> Self {
        self.logo = Some(logo);
        self
    }

    /// Add a resolved inline image (builder style).
    #[must_use]
    pub fn with_image(mut self, image: ImageAsset) -> Self {
        self.images.push(image);
        self
    }

    /// Set the per-page titles used to build the index (builder style).
    #[must_use]
    pub fn with_page_titles(mut self, titles: Vec<String>) -> Self {
        self.page_titles = titles;
        self
    }

    /// Set the layout options (page numbers / index) (builder style).
    #[must_use]
    pub fn with_options(mut self, options: RenderOptions) -> Self {
        self.options = options;
        self
    }

    /// The display title for the page at `index`: its authored title if present,
    /// otherwise a generic `Page N` label.
    #[must_use]
    pub fn page_label(&self, index: usize) -> String {
        self.page_titles
            .get(index)
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map_or_else(|| format!("Page {}", index + 1), str::to_owned)
    }
}

/// A binary image already resolved to bytes: the markdown `src`/`logo` it
/// satisfies, plus the raw bytes. The file extension Typst keys format detection
/// off is derived from the source/filename (defaulting to `.png`).
#[derive(Debug, Clone)]
pub struct ImageAsset {
    /// The markdown image `src` this resolves. For a logo this is unused (the
    /// template references the logo directly).
    pub src: String,
    /// The original filename (used to pick the virtual-file extension Typst
    /// detects the image format from).
    pub filename: String,
    /// The raw image bytes.
    pub bytes: Vec<u8>,
}

impl ImageAsset {
    /// A resolved image for `src`, named `filename`, holding `bytes`.
    #[must_use]
    pub fn new(src: impl Into<String>, filename: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            src: src.into(),
            filename: filename.into(),
            bytes,
        }
    }
}

/// A small mirror of the store's `Branding` (colors + fonts + header/footer),
/// kept here so the render crate stays free of a store dependency. All color
/// fields are CSS-style strings (`#rrggbb`); empty fields fall back to neutral
/// defaults at render time.
#[derive(Debug, Clone, Default)]
pub struct Brand {
    /// The color palette.
    pub colors: Colors,
    /// The typography.
    pub fonts: Fonts,
    /// Optional header text rendered on every page.
    pub header_text: String,
    /// Optional footer text rendered on every page.
    pub footer_text: String,
}

/// The brand color palette (CSS-style strings; `#rrggbb` is what Typst's `rgb`
/// accepts directly).
#[derive(Debug, Clone, Default)]
pub struct Colors {
    /// The dominant brand color (headings, title, rules).
    pub primary: String,
    /// The supporting color.
    pub secondary: String,
    /// The highlight/accent color (links).
    pub accent: String,
    /// Default body-text color.
    pub text: String,
    /// Page/background color.
    pub background: String,
}

/// The brand typography.
#[derive(Debug, Clone, Default)]
pub struct Fonts {
    /// Font family for headings.
    pub heading: String,
    /// Font family for body text.
    pub body: String,
}
