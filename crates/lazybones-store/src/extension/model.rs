//! The durable `Extension` document — an installed WASM backend extension.
//!
//! The extension's `.wasm` component **bytes** live behind the
//! [`BlobStore`](super::super::BlobStore), content-addressed by `wasm_sha256`
//! (the same mechanism as [`Asset`](crate::Asset)); this row is **metadata
//! only** (design §3.5). The metadata mirrors the embedded
//! `lazybones.ext.toml` manifest into the store on install, but the **embedded
//! custom section remains the source of truth** for declared identity/caps —
//! `granted_caps` is the only field whose authority lives here (it is what an
//! admin allowed), and `granted_caps ⊆ requested_caps` is enforced at grant time
//! by `lazybones-ext` (design §3.5).
//!
//! Capabilities are stored as their wire strings (`log`, `store-read`,
//! `http-fetch`, …) rather than a typed enum so the store stays decoupled from
//! the `lazybones-ext` capability vocabulary; `lazybones-ext` owns the typed
//! [`Capability`] parse/validate side.
//!
//! [`Capability`]: https://docs.rs/lazybones-ext

use serde::{Deserialize, Serialize};

/// Where an installed extension came from (design §3.5: "upload, URL, or a
/// future registry").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionSource {
    /// Uploaded directly by an admin (raw `.wasm` bytes).
    Upload,
    /// Fetched from a URL at install time (the URL is retained for re-fetch /
    /// provenance).
    Url(String),
    /// Pulled from a named registry entry (a future install source).
    Registry(String),
}

impl ExtensionSource {
    /// The stored discriminant (`upload` / `url` / `registry`).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            ExtensionSource::Upload => "upload",
            ExtensionSource::Url(_) => "url",
            ExtensionSource::Registry(_) => "registry",
        }
    }

    /// The associated reference (the URL or registry coordinate), if any.
    #[must_use]
    pub fn reference(&self) -> Option<&str> {
        match self {
            ExtensionSource::Upload => None,
            ExtensionSource::Url(r) | ExtensionSource::Registry(r) => Some(r.as_str()),
        }
    }

    /// Reconstruct a source from its stored discriminant + reference. An unknown
    /// kind falls back to [`ExtensionSource::Upload`] (the conservative default —
    /// no re-fetchable origin).
    #[must_use]
    pub fn from_parts(kind: Option<&str>, reference: Option<String>) -> Self {
        match (kind, reference) {
            (Some("url"), Some(r)) => ExtensionSource::Url(r),
            (Some("registry"), Some(r)) => ExtensionSource::Registry(r),
            _ => ExtensionSource::Upload,
        }
    }
}

/// The frontend half of an extension — a Module Federation remote (design §4).
///
/// Mirrors the manifest's `frontend-entry`. The remote's `remoteEntry.js` + its
/// chunks are stored as blobs alongside the `.wasm` and served under
/// `GET /extensions/:id/frontend/*`; the host registers the remote at runtime
/// and mounts its exposed module into the declared UI slots (design §4.1–§4.3).
/// `None` on an [`Extension`] means a backend-only extension with no UI half.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontendDescriptor {
    /// Path (within the frontend blob bundle) of the federated `remoteEntry.js`.
    pub entry: String,
    /// The Module Federation *exposed module* name the host imports to obtain the
    /// remote's mount points (e.g. `./mount`).
    pub exposed_module: String,
    /// Compatible `@lazybones/ext-sdk` semver range the remote declares; the host
    /// surfaces a mismatch and refuses to mount (design §4.3 version
    /// negotiation). `None` = unspecified.
    #[serde(default)]
    pub sdk_range: Option<String>,
    /// The named UI slots this remote registers into (`route`,
    /// `task-detail.tab`, `dashboard.widget`, …; design §4.2).
    #[serde(default)]
    pub slots: Vec<String>,
}

/// An installed backend (and optionally frontend) extension, unique install-wide
/// by `id` (design §3.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extension {
    /// Friendly, unique id (typically minted by the caller, e.g. a ULID).
    pub id: String,
    /// Human name, mirrored from the embedded manifest.
    pub name: String,
    /// Extension version, mirrored from the embedded manifest.
    pub version: String,
    /// The WIT world the component targets (e.g. `extension`), mirrored from the
    /// embedded manifest.
    pub wit_world: String,
    /// The WIT interfaces the component **exports** — the extension points it
    /// implements (`gate-check`, `event-reaction`, …). The registry indexes
    /// extensions by these for dispatch (design §3.2).
    #[serde(default)]
    pub exports: Vec<String>,
    /// Capabilities the manifest **requests** (the declared import surface).
    #[serde(default)]
    pub requested_caps: Vec<String>,
    /// Capabilities an admin **granted**. Always a subset of `requested_caps`
    /// (enforced at grant time by `lazybones-ext`).
    #[serde(default)]
    pub granted_caps: Vec<String>,
    /// Hex SHA-256 of the `.wasm` component bytes — the content address the bytes
    /// are stored under in the [`BlobStore`](super::super::BlobStore).
    pub wasm_sha256: String,
    /// Whether the extension is active. Disabled extensions stay installed but are
    /// not dispatched to.
    pub enabled: bool,
    /// Where the extension was installed from.
    pub source: ExtensionSource,
    /// The frontend remote descriptor, if the extension ships a UI half.
    #[serde(default)]
    pub frontend: Option<FrontendDescriptor>,
    /// RFC3339 install timestamp.
    pub created_at: String,
}

impl Extension {
    /// A freshly installed, **disabled** extension stamped `created_at == now`.
    ///
    /// Installs are disabled-by-default: an admin enables it only after reviewing
    /// the requested capabilities and setting the grants (design §3.3
    /// default-deny). `granted_caps` therefore starts empty.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        wit_world: impl Into<String>,
        exports: Vec<String>,
        requested_caps: Vec<String>,
        wasm_sha256: impl Into<String>,
        source: ExtensionSource,
        now: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            wit_world: wit_world.into(),
            exports,
            requested_caps,
            granted_caps: Vec::new(),
            wasm_sha256: wasm_sha256.into(),
            enabled: false,
            source,
            frontend: None,
            created_at: now.into(),
        }
    }

    /// Attach a frontend descriptor (builder-style).
    #[must_use]
    pub fn with_frontend(mut self, frontend: FrontendDescriptor) -> Self {
        self.frontend = Some(frontend);
        self
    }
}
