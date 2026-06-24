//! Registry + manifest tests (design §3.2, §3.5):
//!
//! - the embedded `lazybones.ext.toml` custom section is read + parsed + validated;
//! - the embedded section WINS on any conflict with the store record (re-review);
//! - `granted_caps ⊆ requested_caps` (and no deferred caps) is enforced;
//! - extensions are indexed by exported interface for dispatch, enabled-only.
//!
//! These build a minimal wasm binary with the manifest written into a custom
//! section by hand, so the test needs no wasm toolchain.

use lazybones_ext::manifest::{Manifest, ManifestError, MANIFEST_SECTION};
use lazybones_ext::registry::{InstallRequest, RecordClaims, Registry, RegistryError};
use lazybones_store::{ExtensionSource, sha256_hex};

/// LEB128-encode an unsigned int (custom-section sizes are LEB128).
fn uleb(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if n == 0 {
            break;
        }
    }
    out
}

/// Build a minimal but well-formed wasm binary (empty module header) carrying the
/// manifest TOML in a `lazybones.ext.toml` custom section.
fn wasm_with_manifest(toml: &str) -> Vec<u8> {
    let mut wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    let name = MANIFEST_SECTION.as_bytes();
    let mut body = Vec::new();
    body.extend_from_slice(&uleb(name.len() as u64));
    body.extend_from_slice(name);
    body.extend_from_slice(toml.as_bytes());

    wasm.push(0x00); // custom section id
    wasm.extend_from_slice(&uleb(body.len() as u64));
    wasm.extend_from_slice(&body);
    wasm
}

const GATE_MANIFEST: &str = r#"
name = "gate-guard"
version = "0.1.0"
wit-world = "extension"
extension-points = ["gate-check"]
capabilities = ["log", "store-read"]

[frontend]
entry = "remoteEntry.js"
exposed-module = "./mount"
sdk-range = "^1.0"
slots = ["task-detail.tab"]
"#;

fn req<'a>(id: &str, wasm: &'a [u8], granted: &[&str]) -> InstallRequest<'a> {
    InstallRequest {
        id: id.to_owned(),
        component: wasm,
        granted_caps: granted.iter().map(|s| (*s).to_owned()).collect(),
        source: ExtensionSource::Upload,
        expected_sha256: None,
        enabled: true,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        record: None,
    }
}

#[test]
fn parses_embedded_manifest_and_indexes_by_export() {
    let wasm = wasm_with_manifest(GATE_MANIFEST);

    // The custom section is read + parsed.
    let manifest = Manifest::from_component(&wasm).expect("manifest");
    assert_eq!(manifest.name, "gate-guard");
    assert_eq!(manifest.extension_points, vec!["gate-check".to_owned()]);
    assert_eq!(manifest.frontend.as_ref().unwrap().exposed_module, "./mount");

    let mut reg = Registry::new();
    reg.install(req("ext-1", &wasm, &["store-read"]))
        .expect("install");

    // Indexed by exported interface, enabled-only.
    let hits = reg.find_by_export("gate-check");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "ext-1");
    assert!(reg.find_by_export("event-reaction").is_empty());

    // `log` is always granted; the explicit grant is honoured.
    let rec = reg.get("ext-1").unwrap();
    assert!(rec.has_capability(lazybones_ext::Capability::Log));
    assert!(rec.has_capability(lazybones_ext::Capability::StoreRead));
    assert!(!rec.has_capability(lazybones_ext::Capability::HttpFetch));

    // The mirrored store record reflects the embedded manifest + grants.
    let stored = rec.to_store_extension();
    assert_eq!(stored.id, "ext-1");
    assert_eq!(stored.version, "0.1.0");
    assert_eq!(stored.wasm_sha256, sha256_hex(&wasm));
    assert_eq!(stored.exports, vec!["gate-check".to_owned()]);
    assert_eq!(stored.requested_caps, vec!["log".to_owned(), "store-read".to_owned()]);
    assert_eq!(stored.granted_caps, vec!["store-read".to_owned()]);
    assert!(stored.enabled);
    assert_eq!(stored.frontend.unwrap().slots, vec!["task-detail.tab".to_owned()]);
}

#[test]
fn disabled_extensions_are_not_dispatched() {
    let wasm = wasm_with_manifest(GATE_MANIFEST);
    let mut reg = Registry::new();
    let mut r = req("ext-1", &wasm, &[]);
    r.enabled = false;
    reg.install(r).expect("install");
    // Indexed but excluded from dispatch until enabled.
    assert!(reg.find_by_export("gate-check").is_empty());
    assert_eq!(reg.all().count(), 1);
}

#[test]
fn grant_must_be_subset_of_requested() {
    let wasm = wasm_with_manifest(GATE_MANIFEST);
    let mut reg = Registry::new();
    // `http-fetch` was never requested by the manifest.
    let err = reg
        .install(req("ext-1", &wasm, &["http-fetch"]))
        .unwrap_err();
    assert!(matches!(err, RegistryError::Grant(_)), "{err:?}");
}

#[test]
fn embedded_section_wins_and_forces_re_review_on_conflict() {
    let wasm = wasm_with_manifest(GATE_MANIFEST);
    let mut reg = Registry::new();
    let mut r = req("ext-1", &wasm, &["store-read"]);
    // A store record that disagrees with the embedded section on requested caps.
    r.record = Some(RecordClaims {
        name: "gate-guard".to_owned(),
        version: "0.1.0".to_owned(),
        wit_world: "extension".to_owned(),
        requested_caps: vec!["log".to_owned(), "http-fetch".to_owned()],
    });
    let err = reg.install(r).unwrap_err();
    assert!(
        matches!(err, RegistryError::ReReviewRequired(_)),
        "expected re-review, got {err:?}"
    );

    // A matching record (order-insensitive) installs cleanly — embedded values used.
    let mut ok = req("ext-1", &wasm, &["store-read"]);
    ok.record = Some(RecordClaims {
        name: "gate-guard".to_owned(),
        version: "0.1.0".to_owned(),
        wit_world: "extension".to_owned(),
        requested_caps: vec!["store-read".to_owned(), "log".to_owned()],
    });
    reg.install(ok).expect("matching record installs");
}

#[test]
fn sha256_mismatch_is_rejected() {
    let wasm = wasm_with_manifest(GATE_MANIFEST);
    let mut reg = Registry::new();
    let mut r = req("ext-1", &wasm, &[]);
    r.expected_sha256 = Some("deadbeef".to_owned());
    let err = reg.install(r).unwrap_err();
    assert!(matches!(err, RegistryError::Sha256Mismatch { .. }), "{err:?}");
}

#[test]
fn deferred_capability_in_manifest_is_invalid() {
    let toml = r#"
name = "writer"
version = "0.1.0"
wit-world = "extension"
extension-points = ["gate-check"]
capabilities = ["store-write"]
"#;
    let wasm = wasm_with_manifest(toml);
    let err = Manifest::from_component(&wasm).unwrap_err();
    assert!(matches!(err, ManifestError::Invalid(_)), "{err:?}");
}

#[test]
fn missing_manifest_section_is_reported() {
    // Bare module header, no custom section.
    let wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let err = Manifest::from_component(&wasm).unwrap_err();
    assert!(matches!(err, ManifestError::Missing(_)), "{err:?}");
}

#[test]
fn duplicate_id_is_rejected() {
    let wasm = wasm_with_manifest(GATE_MANIFEST);
    let mut reg = Registry::new();
    reg.install(req("ext-1", &wasm, &[])).expect("first");
    let err = reg.install(req("ext-1", &wasm, &[])).unwrap_err();
    assert!(matches!(err, RegistryError::AlreadyRegistered(_)), "{err:?}");
}

#[test]
fn remove_drops_from_export_index() {
    let wasm = wasm_with_manifest(GATE_MANIFEST);
    let mut reg = Registry::new();
    reg.install(req("ext-1", &wasm, &[])).expect("install");
    assert_eq!(reg.find_by_export("gate-check").len(), 1);
    let removed = reg.remove("ext-1").expect("removed");
    assert_eq!(removed.id, "ext-1");
    assert!(reg.find_by_export("gate-check").is_empty());
    assert!(reg.get("ext-1").is_none());
}
