//! The durable `Branding` document — a reusable, named brand profile.
//!
//! Branding is **standalone and cross-cutting**, not a document-writer
//! subfeature: the user maintains *many* brand profiles (logo + colors + fonts +
//! header/footer) as a general, app-wide resource. Any feature — the PDF exporter
//! today, app/UI theming and other surfaces later — references a brand by id and
//! resolves it. The document writer is merely the first consumer; a consumer just
//! stores a `branding_id`. Like a [`Skill`](crate::Skill) it is authored once and
//! reused, with no run or lifecycle of its own.

use serde::{Deserialize, Serialize};

/// The brand's color palette, serialized as a single JSON column (like
/// [`SkillAction`](crate::SkillAction)). All fields are CSS-style color strings
/// (`#rrggbb`, `rgb(...)`, a named color, …).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BrandColors {
    /// The dominant brand color.
    #[serde(default)]
    pub primary: String,
    /// The supporting color.
    #[serde(default)]
    pub secondary: String,
    /// The highlight/call-to-action color.
    #[serde(default)]
    pub accent: String,
    /// Default body-text color.
    #[serde(default)]
    pub text: String,
    /// Page/background color.
    #[serde(default)]
    pub background: String,
}

/// The brand's typography, serialized as a single JSON column.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BrandFonts {
    /// Font family for headings.
    #[serde(default)]
    pub heading: String,
    /// Font family for body text.
    #[serde(default)]
    pub body: String,
}

/// A reusable brand profile, unique install-wide by `id` (e.g. `default`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branding {
    /// Friendly, unique id (e.g. `default`, `acme-corp`).
    pub id: String,
    /// Optional project scope; always `None` today (projects land later).
    #[serde(default)]
    pub project: Option<String>,
    /// Human name shown in the brand picker.
    pub name: String,
    /// The [`asset`](crate::Asset) id of the brand's logo, if one is set.
    #[serde(default)]
    pub logo_asset_id: Option<String>,
    /// The brand color palette.
    #[serde(default)]
    pub colors: BrandColors,
    /// The brand typography.
    #[serde(default)]
    pub fonts: BrandFonts,
    /// Optional header text rendered on branded output.
    #[serde(default)]
    pub header_text: String,
    /// Optional footer text rendered on branded output.
    #[serde(default)]
    pub footer_text: String,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Branding {
    /// A freshly authored brand stamped `created_at == updated_at == now`, with
    /// empty colors/fonts and no logo.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, now: impl Into<String>) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            project: None,
            name: name.into(),
            logo_asset_id: None,
            colors: BrandColors::default(),
            fonts: BrandFonts::default(),
            header_text: String::new(),
            footer_text: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Set the color palette (builder style).
    #[must_use]
    pub fn with_colors(mut self, colors: BrandColors) -> Self {
        self.colors = colors;
        self
    }

    /// Set the typography (builder style).
    #[must_use]
    pub fn with_fonts(mut self, fonts: BrandFonts) -> Self {
        self.fonts = fonts;
        self
    }
}
