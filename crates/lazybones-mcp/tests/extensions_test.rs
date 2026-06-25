//! Extension authoring vs. install — the sharp §6.3 split, over the in-process MCP
//! tools.
//!
//! Two guarantees this drives directly the way the streamable-HTTP transport does:
//!
//! 1. **Authoring is allowed.** `extension.scaffold` on a default-`Author` token
//!    writes a `cargo-component` guest skeleton + a `lazybones.ext.toml` manifest
//!    (the manifest it emits parses + validates through `lazybones_ext::Manifest`),
//!    plus a federated-remote skeleton when asked.
//! 2. **Install + grant are loop-only.** `extension.install`/`set_grants`/`enable`/
//!    `disable`/`invoke` refuse an `Author` token with `forbidden` (missing the
//!    loop-only `Extension` capability), and an unauthenticated install with
//!    `unauthorized` — the capability is the gate, exactly as the `/extensions`
//!    routes' `session.require(Capability::Extension)` is. No management profile may
//!    install or grant.

use std::sync::Arc;

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;

use lazybones_auth::{ManagementProfile, ScopedSession};
use lazybones_mcp::args::{
    ExtensionGrantsArgs, ExtensionInstallArgs, ExtensionInvokeArgs, ExtensionScaffoldArgs, IdArgs,
};
use lazybones_mcp::{McpServer, SessionResolver};
use lazybones_store::{StoreEngine, StoreHandle};

/// A one-token registry mapping a single bearer token to its session.
struct OneToken {
    token: String,
    session: ScopedSession,
}

impl SessionResolver for OneToken {
    fn session_for(&self, token: &str) -> Option<ScopedSession> {
        (token == self.token).then(|| self.session.clone())
    }
}

async fn store() -> StoreHandle {
    StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "test-secret-key")
        .await
        .expect("open in-memory store")
}

/// A server whose registry maps `token` to a default-`Author` management session
/// (`Read + Author + Document` — never the loop-only `Extension`).
fn author_server(store: StoreHandle, token: &str) -> McpServer {
    let session = ScopedSession::for_management("ext-test", ManagementProfile::Author);
    let resolver = Arc::new(OneToken {
        token: token.to_owned(),
        session,
    });
    McpServer::new(store, resolver)
}

fn parts_with_token(token: &str) -> http::request::Parts {
    http::Request::builder()
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(())
        .expect("build request")
        .into_parts()
        .0
}

fn parts_without_token() -> http::request::Parts {
    http::Request::builder()
        .body(())
        .expect("build request")
        .into_parts()
        .0
}

/// `extension.scaffold` on an Author token writes the expected guest skeleton +
/// manifest (+ frontend remote), and the manifest it emits parses + validates.
#[tokio::test]
async fn scaffold_writes_guest_skeleton_and_manifest() {
    let store = store().await;
    let token = "author-secret";
    let server = author_server(store, token);

    let dir = tempfile::tempdir().expect("tempdir");
    let args: ExtensionScaffoldArgs = serde_json::from_value(serde_json::json!({
        "dir": dir.path().to_str().unwrap(),
        "id": "gate-strict",
        "name": "Strict Gate",
        "description": "Fails loud diffs.",
        "frontend": true,
    }))
    .expect("ExtensionScaffoldArgs");

    let out = server
        .extension_scaffold(Extension(parts_with_token(token)), Parameters(args))
        .await
        .expect("author token may scaffold");

    // The expected files all landed under `<dir>/<id>/`.
    let root = dir.path().join("gate-strict");
    for rel in [
        "Cargo.toml",
        "src/lib.rs",
        "lazybones.ext.toml",
        ".gitignore",
        "README.md",
        "frontend/package.json",
        "frontend/vite.config.ts",
        "frontend/src/Extension.tsx",
    ] {
        assert!(root.join(rel).is_file(), "expected scaffold to write {rel}");
    }

    // The tool reports the same files it wrote.
    let files = out.0.get("files").and_then(|f| f.as_array()).expect("files array");
    assert!(files.iter().any(|f| f == "lazybones.ext.toml"));
    assert!(files.iter().any(|f| f == "frontend/package.json"));

    // The emitted manifest is install-ready: it parses + validates.
    let manifest_src =
        std::fs::read_to_string(root.join("lazybones.ext.toml")).expect("read manifest");
    let manifest = lazybones_ext::Manifest::parse(&manifest_src).expect("manifest parses");
    assert_eq!(manifest.name, "Strict Gate");
    assert!(manifest.extension_points.contains(&"gate-check".to_owned()));
    assert!(manifest.frontend.is_some(), "frontend remote declared");
}

/// `extension.install` from an Author token is `forbidden` — it needs the loop-only
/// `Extension` capability no management profile holds. It never fetches/installs.
#[tokio::test]
async fn install_forbids_author_token() {
    let store = store().await;
    let token = "author-secret";
    let server = author_server(store, token);

    let args: ExtensionInstallArgs = serde_json::from_value(serde_json::json!({
        "url": "https://example.com/ext.wasm",
    }))
    .expect("ExtensionInstallArgs");

    let err = server
        .extension_install(Extension(parts_with_token(token)), Parameters(args))
        .await
        .err()
        .expect("install must refuse an Author token");
    assert_eq!(err.message, "missing capability: extension");
}

/// Every other gated extension mutator (`set_grants`/`enable`/`disable`/`invoke`)
/// likewise refuses an Author token with the same loop-only `Extension` gate.
#[tokio::test]
async fn grant_enable_disable_invoke_forbid_author_token() {
    let store = store().await;
    let token = "author-secret";
    let server = author_server(store, token);

    macro_rules! forbidden {
        ($call:expr) => {{
            let err = $call.await.err().expect("gated tool must refuse Author token");
            assert_eq!(err.message, "missing capability: extension");
        }};
    }

    let grants: ExtensionGrantsArgs = serde_json::from_value(serde_json::json!({
        "id": "ext-1", "granted_caps": ["log"],
    }))
    .expect("ExtensionGrantsArgs");
    let invoke: ExtensionInvokeArgs = serde_json::from_value(serde_json::json!({
        "id": "ext-1", "export": "gate-check",
    }))
    .expect("ExtensionInvokeArgs");
    let id: IdArgs = serde_json::from_value(serde_json::json!({ "id": "ext-1" })).expect("IdArgs");
    let id2: IdArgs = serde_json::from_value(serde_json::json!({ "id": "ext-1" })).expect("IdArgs");

    forbidden!(server.extension_set_grants(Extension(parts_with_token(token)), Parameters(grants)));
    forbidden!(server.extension_enable(Extension(parts_with_token(token)), Parameters(id)));
    forbidden!(server.extension_disable(Extension(parts_with_token(token)), Parameters(id2)));
    forbidden!(server.extension_invoke(Extension(parts_with_token(token)), Parameters(invoke)));
}

/// An unauthenticated install is `unauthorized` (a mutator never falls through to
/// the read path) — and it never reaches the registry or the network.
#[tokio::test]
async fn unauthenticated_install_is_unauthorized() {
    let store = store().await;
    let server = author_server(store, "author-secret");

    let args: ExtensionInstallArgs = serde_json::from_value(serde_json::json!({
        "url": "https://example.com/ext.wasm",
    }))
    .expect("ExtensionInstallArgs");

    let err = server
        .extension_install(Extension(parts_without_token()), Parameters(args))
        .await
        .err()
        .expect("an unauthenticated install must refuse");
    assert_eq!(err.message, "unauthorized");
}
