//! Document/branding/asset tools over the in-process MCP router (design §6.2).
//!
//! Two guarantees the §6.2 surface must hold:
//! 1. **`Document`-gated mutators 403 without the capability.** A `ReadOnly` token
//!    (holds only `Read`, never `Document`) must be refused with `forbidden` on
//!    every authoring/publish verb, and an unauthenticated call with `unauthorized`.
//! 2. **Author → render round-trips.** A `Document`-holding token creates a
//!    document, adds a page, and `document.render` returns HTML carrying that page.
//!
//! These drive the real tool methods the way the streamable-HTTP transport does,
//! against an in-memory store.

use std::sync::Arc;

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;

use lazybones_auth::{ManagementProfile, ScopedSession};
use lazybones_mcp::args::{
    BrandingArgs, DocumentAddPageArgs, DocumentAddSourceArgs, DocumentAttachReferenceArgs,
    DocumentCreateArgs, DocumentPublishArgs, DocumentRenderArgs, DocumentSetRepoArgs,
    DocumentUpdateArgs,
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

/// A server whose registry maps `token` to a management session of `profile`.
fn server_with(store: StoreHandle, token: &str, profile: ManagementProfile) -> McpServer {
    let session = ScopedSession::for_management("doc-test", profile);
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

fn from_json<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("deserialize args")
}

/// Every `Document`-gated mutator refuses a `ReadOnly` token with `forbidden`
/// (naming the missing `document` grant) — the capability is the gate, exactly as
/// the REST routes' `session.require(Document, …)` is.
#[tokio::test]
async fn document_gated_tools_forbid_readonly_token() {
    let store = store().await;
    let token = "readonly-secret";
    let server = server_with(store, token, ManagementProfile::ReadOnly);

    macro_rules! forbidden {
        ($call:expr) => {{
            let err = $call
                .await
                .err()
                .expect("a Document-gated tool must refuse a ReadOnly token");
            assert_eq!(
                err.message, "missing capability: document",
                "expected a `document` refusal"
            );
        }};
    }

    forbidden!(server.document_create(
        Extension(parts_with_token(token)),
        Parameters(from_json::<DocumentCreateArgs>(
            serde_json::json!({ "id": "d1", "title": "Doc" })
        ))
    ));
    forbidden!(server.document_update(
        Extension(parts_with_token(token)),
        Parameters(from_json::<DocumentUpdateArgs>(
            serde_json::json!({ "id": "d1", "title": "Doc" })
        ))
    ));
    forbidden!(server.document_add_page(
        Extension(parts_with_token(token)),
        Parameters(from_json::<DocumentAddPageArgs>(
            serde_json::json!({ "document_id": "d1", "title": "P", "body": "b" })
        ))
    ));
    forbidden!(server.document_attach_reference(
        Extension(parts_with_token(token)),
        Parameters(from_json::<DocumentAttachReferenceArgs>(
            serde_json::json!({ "document_id": "d1", "reference_id": "r1" })
        ))
    ));
    forbidden!(server.document_add_source(
        Extension(parts_with_token(token)),
        Parameters(from_json::<DocumentAddSourceArgs>(
            serde_json::json!({ "document_id": "d1", "url": "https://x" })
        ))
    ));
    forbidden!(server.branding_create(
        Extension(parts_with_token(token)),
        Parameters(from_json::<BrandingArgs>(
            serde_json::json!({ "id": "b1", "name": "Brand" })
        ))
    ));
    forbidden!(server.document_set_repo(
        Extension(parts_with_token(token)),
        Parameters(from_json::<DocumentSetRepoArgs>(
            serde_json::json!({ "id": "d1", "repo": "/tmp/x", "output_path": "docs/d1.md" })
        ))
    ));
    forbidden!(server.document_publish(
        Extension(parts_with_token(token)),
        Parameters(from_json::<DocumentPublishArgs>(
            serde_json::json!({ "id": "d1" })
        ))
    ));
}

/// An unauthenticated `Document` mutator is refused as `unauthorized` (a mutator
/// never falls through to the read path) — and it never reaches the store.
#[tokio::test]
async fn unauthenticated_document_mutator_is_unauthorized() {
    let store = store().await;
    let server = server_with(store, "any-secret", ManagementProfile::Author);

    let err = server
        .document_create(
            Extension(parts_without_token()),
            Parameters(from_json::<DocumentCreateArgs>(
                serde_json::json!({ "id": "d1", "title": "Doc" }),
            )),
        )
        .await
        .err()
        .expect("an unauthenticated mutator must refuse");
    assert_eq!(err.message, "unauthorized");
}

/// The author → render round-trip: a `Document`-holding token (the default `Author`
/// profile) creates a document, adds a page, and `document.render` returns HTML that
/// carries the page body. Open reads need no token, so render is called without one.
#[tokio::test]
async fn author_then_render_round_trip() {
    let store = store().await;
    let token = "author-secret";
    let server = server_with(store, token, ManagementProfile::Author);

    // Author the document.
    server
        .document_create(
            Extension(parts_with_token(token)),
            Parameters(from_json::<DocumentCreateArgs>(serde_json::json!({
                "id": "guide",
                "title": "Field Guide",
            }))),
        )
        .await
        .expect("document.create with a Document token");

    // Add a page with a distinctive body we can find in the rendered HTML.
    server
        .document_add_page(
            Extension(parts_with_token(token)),
            Parameters(from_json::<DocumentAddPageArgs>(serde_json::json!({
                "document_id": "guide",
                "title": "Intro",
                "body": "The quick brown fox jumps.",
            }))),
        )
        .await
        .expect("document.add_page with a Document token");

    // Render is an open read — no token needed. Assert the HTML carries the page.
    let rendered = server
        .document_render(Parameters(from_json::<DocumentRenderArgs>(
            serde_json::json!({ "id": "guide" }),
        )))
        .await
        .expect("document.render");
    let html = rendered.0.get("html").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        html.contains("quick brown fox"),
        "rendered HTML should carry the authored page body, got: {html}"
    );
}
