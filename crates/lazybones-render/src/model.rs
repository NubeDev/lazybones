//! The pure inputs `lazybones-render` renders from.
//!
//! The crate has **no store dependency**: the API assembles a document (merging
//! its reusable `reference` pages into one markdown blob and fetching the logo +
//! inline image bytes from the `BlobStore`) and hands the result here as plain
//! values. That keeps rendering pure and unit-testable. The store's `Branding`
//! type is intentionally mirrored as a small local [`Brand`] so this crate never
//! pulls in `lazybones-store`.

/// A fully-assembled document ready to render: its title, the resolved markdown
/// (body + merged references), the resolved brand profile, and any binary
/// images (logo + inline) already fetched from the blob store.
#[derive(Debug, Clone, Default)]
pub struct Assembled {
    /// The document title (rendered as the cover heading).
    pub title: String,
    /// The assembled markdown: the document body followed by each merged
    /// reference page, in attach order.
    pub markdown: String,
    /// The resolved brand profile (colors, fonts, header/footer). `Default` is a
    /// neutral, unbranded look.
    pub brand: Brand,
    /// The brand logo, already fetched from the blob store, if the brand sets one.
    pub logo: Option<ImageAsset>,
    /// Inline images referenced by the markdown (`![alt](src)`), each already
    /// fetched from the blob store and keyed by the markdown `src` it resolves.
    pub images: Vec<ImageAsset>,
}

impl Assembled {
    /// A bare assembled document with just a title and markdown and the default
    /// (unbranded) look. Builder-style setters layer brand/logo/images on top.
    #[must_use]
    pub fn new(title: impl Into<String>, markdown: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            markdown: markdown.into(),
            ..Self::default()
        }
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
