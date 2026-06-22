//! The persisted shape of a [`Branding`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Branding`]
//! carries a plain string id. Colors/fonts are JSON-encoded into a single string
//! column each (like [`SkillRow.action`](crate::skill)) so the row stays flat
//! regardless of their JSON shape. `Option` columns keep the row
//! forward-compatible: a field added later reads back as `None` on older rows.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::{BrandColors, BrandFonts, Branding};

/// The table brand profiles live in.
pub(crate) const BRANDING_TABLE: &str = "branding";

/// SurrealDB-facing brand profile: the reserved `id` thing plus the brand fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct BrandingRow {
    pub(crate) id: RecordId,
    /// Optional project scope; `None` today.
    pub(crate) project: Option<String>,
    pub(crate) name: String,
    pub(crate) logo_asset_id: Option<String>,
    /// JSON-serialized [`BrandColors`]; `None` (or unparseable) reads back as the
    /// default palette.
    pub(crate) colors: Option<String>,
    /// JSON-serialized [`BrandFonts`].
    pub(crate) fonts: Option<String>,
    pub(crate) header_text: Option<String>,
    pub(crate) footer_text: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl BrandingRow {
    /// Project a domain [`Branding`] into its persisted row.
    pub(crate) fn from_branding(b: &Branding) -> Self {
        Self {
            id: RecordId::new(BRANDING_TABLE, b.id.as_str()),
            project: b.project.clone(),
            name: b.name.clone(),
            logo_asset_id: b.logo_asset_id.clone(),
            colors: serde_json::to_string(&b.colors).ok(),
            fonts: serde_json::to_string(&b.fonts).ok(),
            header_text: Some(b.header_text.clone()),
            footer_text: Some(b.footer_text.clone()),
            created_at: Some(b.created_at.clone()),
            updated_at: Some(b.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Branding`].
    pub(crate) fn into_branding(self) -> Branding {
        Branding {
            id: branding_key(&self.id),
            project: self.project,
            name: self.name,
            logo_asset_id: self.logo_asset_id,
            colors: self
                .colors
                .and_then(|s| serde_json::from_str::<BrandColors>(&s).ok())
                .unwrap_or_default(),
            fonts: self
                .fonts
                .and_then(|s| serde_json::from_str::<BrandFonts>(&s).ok())
                .unwrap_or_default(),
            header_text: self.header_text.unwrap_or_default(),
            footer_text: self.footer_text.unwrap_or_default(),
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a brand id's key (the part after `branding:`).
fn branding_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
