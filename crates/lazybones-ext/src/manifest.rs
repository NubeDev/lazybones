//! The extension manifest (`lazybones.ext.toml`) and its embedded custom section.
//!
//! The manifest declares an extension's identity, the extension points it
//! implements, the capabilities it requests, and its optional frontend remote
//! (design §3.5). It is authored as TOML, **embedded into the component as a
//! custom section**, and mirrored into the [`Extension`] store record on install.
//!
//! Two homes, one authority (design §3.5): the **embedded custom section is the
//! source of truth** for declared identity/caps (and, once signing lands, it is
//! the signed artifact); the store record's `granted_caps` is the only field the
//! admin owns. On any conflict the embedded section wins — see
//! [`crate::registry`].
//!
//! [`Extension`]: lazybones_store::Extension

use serde::Deserialize;

use crate::capability::{Capability, CapabilityError};

/// The wasm **custom section** name the manifest is embedded under. A component is
/// installed with its `lazybones.ext.toml` written verbatim into a custom section
/// of this name; [`extract_embedded`] reads it back out.
pub const MANIFEST_SECTION: &str = "lazybones.ext.toml";

/// Failures parsing or locating the manifest.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// The component carried no `lazybones.ext.toml` custom section.
    #[error("no embedded manifest custom section ({0:?}) found in component")]
    Missing(&'static str),

    /// The component bytes could not be parsed as a wasm module/component to scan
    /// for custom sections.
    #[error("failed to scan component for custom sections: {0}")]
    Scan(String),

    /// The embedded section was not valid UTF-8 TOML / did not match the schema.
    #[error("failed to parse embedded manifest: {0}")]
    Parse(String),

    /// A declared capability string was not a recognised capability.
    #[error(transparent)]
    Capability(#[from] CapabilityError),

    /// The manifest was structurally valid but semantically rejected (e.g. it
    /// requests a deferred capability, or declares no extension points).
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

/// The optional frontend half of an extension (design §4), as authored in TOML.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct FrontendManifest {
    /// Path of the federated `remoteEntry.js` within the frontend bundle.
    pub entry: String,
    /// The Module Federation exposed-module name the host imports.
    pub exposed_module: String,
    /// Compatible `@lazybones/ext-sdk` semver range (design §4.3).
    #[serde(default)]
    pub sdk_range: Option<String>,
    /// UI slots the remote registers into (design §4.2).
    #[serde(default)]
    pub slots: Vec<String>,
}

/// A parsed, schema-valid `lazybones.ext.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Manifest {
    /// Human / package name.
    pub name: String,
    /// Extension version.
    pub version: String,
    /// The WIT world the component targets (e.g. `extension`).
    pub wit_world: String,
    /// The WIT interfaces the component exports — the extension points it
    /// implements. The registry indexes by these for dispatch (design §3.2).
    pub extension_points: Vec<String>,
    /// Capabilities the extension requests (its declared import surface). Stored
    /// as wire strings; validated into [`Capability`] by [`Self::requested_caps`].
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The frontend remote, if any.
    #[serde(default)]
    pub frontend: Option<FrontendManifest>,
}

impl Manifest {
    /// Parse a manifest from raw TOML text and validate it.
    ///
    /// # Errors
    /// [`ManifestError::Parse`] if the TOML is malformed/mismatched,
    /// [`ManifestError::Capability`] if a capability string is unknown, or
    /// [`ManifestError::Invalid`] if it requests a deferred capability or declares
    /// no extension points.
    pub fn parse(toml_str: &str) -> Result<Self, ManifestError> {
        let manifest: Manifest =
            toml::from_str(toml_str).map_err(|e| ManifestError::Parse(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Locate and parse the manifest embedded in a component's custom section.
    ///
    /// # Errors
    /// [`ManifestError::Scan`] if the bytes aren't a parseable wasm component,
    /// [`ManifestError::Missing`] if no manifest section is present, plus anything
    /// [`Self::parse`] raises.
    pub fn from_component(component: &[u8]) -> Result<Self, ManifestError> {
        let raw = extract_embedded(component)?
            .ok_or(ManifestError::Missing(MANIFEST_SECTION))?;
        let text =
            std::str::from_utf8(&raw).map_err(|e| ManifestError::Parse(e.to_string()))?;
        Self::parse(text)
    }

    /// The requested capabilities as typed [`Capability`] values.
    ///
    /// # Errors
    /// [`ManifestError::Capability`] if any string is not a known capability.
    pub fn requested_caps(&self) -> Result<Vec<Capability>, ManifestError> {
        self.capabilities
            .iter()
            .map(|c| Capability::parse(c).map_err(ManifestError::from))
            .collect()
    }

    /// Semantic validation beyond the TOML schema: at least one extension point,
    /// every capability known, and **no deferred capability requested** (v1 ships
    /// `log` / `store-read` / `http-fetch` only — design §3.3/§3.7).
    fn validate(&self) -> Result<(), ManifestError> {
        if self.name.trim().is_empty() {
            return Err(ManifestError::Invalid("name must not be empty".into()));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::Invalid("version must not be empty".into()));
        }
        if self.wit_world.trim().is_empty() {
            return Err(ManifestError::Invalid("wit-world must not be empty".into()));
        }
        if self.extension_points.is_empty() {
            return Err(ManifestError::Invalid(
                "manifest declares no extension-points".into(),
            ));
        }
        for cap in self.requested_caps()? {
            if !cap.is_available() {
                return Err(ManifestError::Invalid(format!(
                    "capability `{cap}` is deferred and cannot be requested in v1"
                )));
            }
        }
        Ok(())
    }
}

/// Read the raw bytes of the `lazybones.ext.toml` custom section out of a wasm
/// component, or `None` if it has no such section.
///
/// Scans the component's top-level custom sections (where install embeds the
/// manifest). Uses the same binary-format reader family as the engine that loads
/// the component, so it stays in lock-step across a Wasmtime bump.
///
/// # Errors
/// [`ManifestError::Scan`] if the bytes are not a parseable wasm binary.
pub fn extract_embedded(component: &[u8]) -> Result<Option<Vec<u8>>, ManifestError> {
    use wasmparser::{Parser, Payload};

    for payload in Parser::new(0).parse_all(component) {
        let payload = payload.map_err(|e| ManifestError::Scan(e.to_string()))?;
        if let Payload::CustomSection(reader) = payload
            && reader.name() == MANIFEST_SECTION
        {
            return Ok(Some(reader.data().to_vec()));
        }
    }
    Ok(None)
}
