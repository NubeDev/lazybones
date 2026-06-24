//! Extension registry.
//!
//! On install, the registry reads the manifest from the component's embedded
//! custom section, validates it, reconciles it against the store record, enforces
//! the capability grant policy, and indexes the result by exported WIT interface
//! so dispatch can "find all gate-check extensions, invoke in order" (design
//! §3.2, §3.5).
//!
//! ## The embedded section wins (design §3.5)
//!
//! The manifest has two homes: the **embedded custom section** (source of truth
//! for declared identity/caps — and, once signing lands, the signed artifact) and
//! the **store record** (source of truth only for `granted_caps`, what an admin
//! allowed). On any conflict between the embedded section and a supplied record's
//! declared identity/`requested_caps`, the embedded section wins **and the
//! mismatch forces re-review** — [`Registry::install`] refuses with
//! [`RegistryError::ReReviewRequired`] rather than silently trusting the record.
//!
//! ## Grants are a subset of requests
//!
//! `granted_caps ⊆ requested_caps` is enforced at install/grant time
//! ([`crate::capability::validate_grant`]); `log` is always granted and deferred
//! capabilities can never be.

use std::collections::HashMap;

use lazybones_store::{Extension, ExtensionSource, FrontendDescriptor, sha256_hex};

use crate::capability::{Capability, GrantError, validate_grant};
use crate::manifest::{Manifest, ManifestError};

/// The declared half of a store [`Extension`] record, cross-checked against the
/// embedded manifest at install time. The embedded section wins on any conflict.
#[derive(Debug, Clone)]
pub struct RecordClaims {
    /// The record's declared name.
    pub name: String,
    /// The record's declared version.
    pub version: String,
    /// The record's declared WIT world.
    pub wit_world: String,
    /// The record's declared requested capabilities (wire strings).
    pub requested_caps: Vec<String>,
}

impl RecordClaims {
    /// Derive the claims from an existing store [`Extension`] record.
    #[must_use]
    pub fn from_extension(e: &Extension) -> Self {
        Self {
            name: e.name.clone(),
            version: e.version.clone(),
            wit_world: e.wit_world.clone(),
            requested_caps: e.requested_caps.clone(),
        }
    }
}

/// Everything needed to install one extension into the registry.
pub struct InstallRequest<'a> {
    /// The install-wide unique id to register under.
    pub id: String,
    /// The raw `.wasm` component bytes (carrying the embedded manifest).
    pub component: &'a [u8],
    /// Capabilities the admin grants (wire strings); validated `⊆ requested`.
    pub granted_caps: Vec<String>,
    /// Where the extension came from.
    pub source: ExtensionSource,
    /// An optionally pre-computed/asserted content digest. When set, the actual
    /// hash of `component` must match it or install fails — integrity check for an
    /// upload-by-URL or a re-install against an existing record.
    pub expected_sha256: Option<String>,
    /// Whether the extension is enabled on install (default-deny installs pass
    /// `false` and enable after review).
    pub enabled: bool,
    /// RFC3339 install timestamp for the produced store record.
    pub created_at: String,
    /// A prior/declared store record to reconcile against. The embedded section
    /// wins on any conflict; a conflict forces re-review (design §3.5).
    pub record: Option<RecordClaims>,
}

/// Failures installing or resolving an extension.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RegistryError {
    /// The embedded manifest was missing, malformed, or semantically invalid.
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    /// The supplied `wasm_sha256` did not match the actual component bytes.
    #[error("wasm sha256 mismatch: record claims {claimed}, bytes hash to {actual}")]
    Sha256Mismatch {
        /// The sha the caller/record asserted.
        claimed: String,
        /// The sha the bytes actually hash to (the content address bytes are stored under).
        actual: String,
    },

    /// The store record's declared identity/caps conflict with the embedded
    /// (authoritative) section. The embedded values win; re-review is required
    /// before trusting the install (design §3.5).
    #[error("manifest/record conflict — re-review required: {0}")]
    ReReviewRequired(String),

    /// A granted capability violated the grant policy (`⊆ requested`, no deferred).
    #[error(transparent)]
    Grant(#[from] GrantError),

    /// An extension with this id is already registered.
    #[error("extension already registered: {0}")]
    AlreadyRegistered(String),
}

/// A validated, installed extension: the **authoritative** embedded manifest plus
/// the admin grant and the content address of its bytes.
#[derive(Debug, Clone)]
pub struct ExtensionRecord {
    /// Install-wide unique id.
    pub id: String,
    /// The embedded manifest — the source of truth for identity/caps (§3.5).
    pub manifest: Manifest,
    /// The capabilities the admin granted (already validated `⊆ requested`).
    pub granted_caps: Vec<Capability>,
    /// The hex SHA-256 the `.wasm` bytes are stored under in the blob store.
    pub wasm_sha256: String,
    /// Where the extension came from.
    pub source: ExtensionSource,
    /// Whether the extension is active (eligible for dispatch).
    pub enabled: bool,
    /// RFC3339 install timestamp.
    pub created_at: String,
}

impl ExtensionRecord {
    /// The WIT interfaces this extension exports — the extension points it
    /// implements, indexed by the registry for dispatch.
    #[must_use]
    pub fn exports(&self) -> &[String] {
        &self.manifest.extension_points
    }

    /// Whether the (always-`log`-plus-granted) effective grant includes `cap`.
    #[must_use]
    pub fn has_capability(&self, cap: Capability) -> bool {
        cap.is_always_granted() || self.granted_caps.contains(&cap)
    }

    /// Project this validated record into a durable store [`Extension`] — the
    /// metadata mirror written on install (design §3.5). All identity/caps fields
    /// come from the **embedded manifest** (the winner); only `granted_caps` and
    /// `enabled` reflect the admin's decision.
    #[must_use]
    pub fn to_store_extension(&self) -> Extension {
        let mut ext = Extension::new(
            self.id.clone(),
            self.manifest.name.clone(),
            self.manifest.version.clone(),
            self.manifest.wit_world.clone(),
            self.manifest.extension_points.clone(),
            self.manifest.capabilities.clone(),
            self.wasm_sha256.clone(),
            self.source.clone(),
            self.created_at.clone(),
        );
        ext.granted_caps = self.granted_caps.iter().map(|c| c.as_str().to_owned()).collect();
        ext.enabled = self.enabled;
        if let Some(f) = &self.manifest.frontend {
            ext = ext.with_frontend(FrontendDescriptor {
                entry: f.entry.clone(),
                exposed_module: f.exposed_module.clone(),
                sdk_range: f.sdk_range.clone(),
                slots: f.slots.clone(),
            });
        }
        ext
    }
}

/// The in-memory registry of installed extensions, indexed by exported interface.
#[derive(Debug, Default)]
pub struct Registry {
    by_id: HashMap<String, ExtensionRecord>,
    /// exported interface name → the ids of extensions exporting it.
    by_export: HashMap<String, Vec<String>>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install an extension: parse + validate the embedded manifest, reconcile it
    /// against any supplied record (embedded wins; conflict ⇒ re-review), enforce
    /// the grant policy, store it, and index it by exported interface.
    ///
    /// Returns a reference to the newly registered record.
    ///
    /// # Errors
    /// See [`RegistryError`]: a bad/absent manifest, a sha mismatch, a
    /// manifest/record conflict, a grant-policy violation, or a duplicate id.
    pub fn install(&mut self, req: InstallRequest<'_>) -> Result<&ExtensionRecord, RegistryError> {
        if self.by_id.contains_key(&req.id) {
            return Err(RegistryError::AlreadyRegistered(req.id));
        }

        // The embedded section is authoritative — parse + validate it first.
        let manifest = Manifest::from_component(req.component)?;

        // Content-address the bytes; the blob store keys on this sha (design §3.5).
        let actual_sha = sha256_hex(req.component);
        if let Some(claimed) = &req.expected_sha256
            && claimed != &actual_sha
        {
            return Err(RegistryError::Sha256Mismatch {
                claimed: claimed.clone(),
                actual: actual_sha,
            });
        }

        // Reconcile against the supplied record. The embedded section WINS; any
        // declared-identity / requested-caps divergence forces re-review.
        if let Some(record) = &req.record {
            reconcile(&manifest, record)?;
        }

        // Enforce `granted ⊆ requested` (+ no deferred caps).
        let requested = manifest.requested_caps()?;
        let granted = parse_caps(&req.granted_caps)?;
        validate_grant(&requested, &granted)?;

        let record = ExtensionRecord {
            id: req.id.clone(),
            manifest,
            granted_caps: granted,
            wasm_sha256: actual_sha,
            source: req.source,
            enabled: req.enabled,
            created_at: req.created_at,
        };

        // Index by exported interface for dispatch.
        for export in record.exports() {
            self.by_export
                .entry(export.clone())
                .or_default()
                .push(req.id.clone());
        }
        self.by_id.insert(req.id.clone(), record);
        Ok(&self.by_id[&req.id])
    }

    /// Look up a registered extension by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ExtensionRecord> {
        self.by_id.get(id)
    }

    /// Every registered extension (enabled or not).
    pub fn all(&self) -> impl Iterator<Item = &ExtensionRecord> {
        self.by_id.values()
    }

    /// All **enabled** extensions exporting `interface`, in install order — the
    /// dispatch set for that extension point (design §3.2). Disabled extensions
    /// are indexed but excluded here.
    #[must_use]
    pub fn find_by_export(&self, interface: &str) -> Vec<&ExtensionRecord> {
        self.by_export
            .get(interface)
            .into_iter()
            .flatten()
            .filter_map(|id| self.by_id.get(id))
            .filter(|r| r.enabled)
            .collect()
    }

    /// Flip an installed extension's `enabled` flag in place, keeping the
    /// in-memory dispatch index in lock-step with an admin's enable/disable
    /// decision persisted in the store (design §3.6). Returns whether the id was
    /// registered; a no-op `false` is fine (the store stays authoritative and the
    /// registry is rebuilt from it on boot).
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        match self.by_id.get_mut(id) {
            Some(record) => {
                record.enabled = enabled;
                true
            }
            None => false,
        }
    }

    /// Replace an installed extension's granted capabilities in place, mirroring an
    /// admin's grant decision (design §3.6). The `granted ⊆ requested` policy is
    /// enforced by the caller at grant time; this only keeps the dispatch index's
    /// effective grant current. Returns whether the id was registered.
    pub fn set_grants(&mut self, id: &str, granted: Vec<Capability>) -> bool {
        match self.by_id.get_mut(id) {
            Some(record) => {
                record.granted_caps = granted;
                true
            }
            None => false,
        }
    }

    /// Remove an extension from the registry (and its export index). Returns the
    /// removed record, if any.
    pub fn remove(&mut self, id: &str) -> Option<ExtensionRecord> {
        let record = self.by_id.remove(id)?;
        for export in record.exports() {
            if let Some(ids) = self.by_export.get_mut(export) {
                ids.retain(|other| other != id);
                if ids.is_empty() {
                    self.by_export.remove(export);
                }
            }
        }
        Some(record)
    }
}

/// Reconcile the embedded (authoritative) manifest against a store record.
/// The embedded values win; any divergence in declared identity or requested
/// capabilities is a re-review trigger (design §3.5).
fn reconcile(manifest: &Manifest, record: &RecordClaims) -> Result<(), RegistryError> {
    let mut conflicts = Vec::new();
    if manifest.name != record.name {
        conflicts.push(format!(
            "name (embedded `{}` vs record `{}`)",
            manifest.name, record.name
        ));
    }
    if manifest.version != record.version {
        conflicts.push(format!(
            "version (embedded `{}` vs record `{}`)",
            manifest.version, record.version
        ));
    }
    if manifest.wit_world != record.wit_world {
        conflicts.push(format!(
            "wit-world (embedded `{}` vs record `{}`)",
            manifest.wit_world, record.wit_world
        ));
    }
    if !same_set(&manifest.capabilities, &record.requested_caps) {
        conflicts.push(format!(
            "requested-caps (embedded {:?} vs record {:?})",
            manifest.capabilities, record.requested_caps
        ));
    }
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(RegistryError::ReReviewRequired(conflicts.join("; ")))
    }
}

/// Whether two capability string lists denote the same set (order-insensitive).
fn same_set(a: &[String], b: &[String]) -> bool {
    let mut a: Vec<&str> = a.iter().map(String::as_str).collect();
    let mut b: Vec<&str> = b.iter().map(String::as_str).collect();
    a.sort_unstable();
    a.dedup();
    b.sort_unstable();
    b.dedup();
    a == b
}

/// Parse a list of capability wire strings, surfacing the first unknown one.
fn parse_caps(caps: &[String]) -> Result<Vec<Capability>, ManifestError> {
    caps.iter()
        .map(|c| Capability::parse(c).map_err(ManifestError::Capability))
        .collect()
}
