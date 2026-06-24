//! The persisted shape of an [`Extension`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Extension`]
//! carries a plain string id. The [`ExtensionSource`] enum is flattened to a
//! `source_kind` + `source_ref` pair, and the [`FrontendDescriptor`] is a nested
//! sub-object. `Option` columns keep the row forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::{Extension, ExtensionSource, FrontendDescriptor};

/// The table extension metadata lives in.
pub(crate) const EXTENSION_TABLE: &str = "extension";

/// SurrealDB-facing frontend descriptor sub-object.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct FrontendRow {
    pub(crate) entry: Option<String>,
    pub(crate) exposed_module: Option<String>,
    pub(crate) sdk_range: Option<String>,
    pub(crate) slots: Option<Vec<String>>,
}

/// SurrealDB-facing extension: the reserved `id` thing plus the metadata mirrored
/// from the embedded manifest, the admin grants, and the blob content address.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct ExtensionRow {
    pub(crate) id: RecordId,
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) wit_world: Option<String>,
    pub(crate) exports: Option<Vec<String>>,
    pub(crate) requested_caps: Option<Vec<String>>,
    pub(crate) granted_caps: Option<Vec<String>>,
    pub(crate) wasm_sha256: Option<String>,
    pub(crate) enabled: Option<bool>,
    pub(crate) source_kind: Option<String>,
    pub(crate) source_ref: Option<String>,
    pub(crate) frontend: Option<FrontendRow>,
    pub(crate) created_at: Option<String>,
}

impl ExtensionRow {
    /// Project a domain [`Extension`] into its persisted row.
    pub(crate) fn from_extension(e: &Extension) -> Self {
        Self {
            id: RecordId::new(EXTENSION_TABLE, e.id.as_str()),
            name: Some(e.name.clone()),
            version: Some(e.version.clone()),
            wit_world: Some(e.wit_world.clone()),
            exports: Some(e.exports.clone()),
            requested_caps: Some(e.requested_caps.clone()),
            granted_caps: Some(e.granted_caps.clone()),
            wasm_sha256: Some(e.wasm_sha256.clone()),
            enabled: Some(e.enabled),
            source_kind: Some(e.source.kind().to_owned()),
            source_ref: e.source.reference().map(ToOwned::to_owned),
            frontend: e.frontend.as_ref().map(|f| FrontendRow {
                entry: Some(f.entry.clone()),
                exposed_module: Some(f.exposed_module.clone()),
                sdk_range: f.sdk_range.clone(),
                slots: Some(f.slots.clone()),
            }),
            created_at: Some(e.created_at.clone()),
        }
    }

    /// Reconstruct the domain [`Extension`].
    pub(crate) fn into_extension(self) -> Extension {
        Extension {
            id: extension_key(&self.id),
            name: self.name.unwrap_or_default(),
            version: self.version.unwrap_or_default(),
            wit_world: self.wit_world.unwrap_or_default(),
            exports: self.exports.unwrap_or_default(),
            requested_caps: self.requested_caps.unwrap_or_default(),
            granted_caps: self.granted_caps.unwrap_or_default(),
            wasm_sha256: self.wasm_sha256.unwrap_or_default(),
            enabled: self.enabled.unwrap_or(false),
            source: ExtensionSource::from_parts(self.source_kind.as_deref(), self.source_ref),
            frontend: self.frontend.map(|f| FrontendDescriptor {
                entry: f.entry.unwrap_or_default(),
                exposed_module: f.exposed_module.unwrap_or_default(),
                sdk_range: f.sdk_range,
                slots: f.slots.unwrap_or_default(),
            }),
            created_at: self.created_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of an extension id's key (the part after `extension:`).
fn extension_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
