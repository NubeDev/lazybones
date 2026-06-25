//! Author-a-workflow round-trip over the in-process MCP router (design §6.1, task
//! `mcp-spike`).
//!
//! Drives the real [`McpServer::workflow_create`] tool the way the streamable-HTTP
//! transport does — a request's [`http::request::Parts`] (carrying the bearer token)
//! plus the typed args — against an in-memory store, with no HTTP-to-self. It proves
//! the P0 contract end to end:
//!
//! - a minted **`Author`** token creates a workflow that then exists via
//!   [`StoreHandle::get_run`] (the same store boundary the REST surface reads), and
//! - a **no-token** `workflow.create` is refused as `unauthorized` and writes
//!   nothing — authoring is gated by the capability exactly as REST is (§3).

use std::sync::Arc;

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;

use lazybones_auth::{ManagementProfile, ScopedSession};
use lazybones_mcp::McpServer;
use lazybones_mcp::SessionResolver;
use lazybones_mcp::args::WorkflowCreateArgs;
use lazybones_store::{StoreEngine, StoreHandle};

/// A one-token registry: the MCP twin of the API's token map, so the in-process
/// server authenticates a bearer token to its session exactly like a REST request.
struct OneToken {
    token: String,
    session: ScopedSession,
}

impl SessionResolver for OneToken {
    fn session_for(&self, token: &str) -> Option<ScopedSession> {
        (token == self.token).then(|| self.session.clone())
    }
}

/// A fresh in-memory store — the same `StoreEngine::Memory` the store's own tests use.
async fn store() -> StoreHandle {
    StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "test-secret-key")
        .await
        .expect("open in-memory store")
}

/// Build a server whose registry maps `token` to a freshly minted `Author` session
/// (the default management grant: `Read + Author + Document`).
fn server_with_author_token(store: StoreHandle, token: &str) -> McpServer {
    let session = ScopedSession::for_management("mcp-spike", ManagementProfile::Author);
    let resolver = Arc::new(OneToken {
        token: token.to_owned(),
        session,
    });
    McpServer::new(store, resolver)
}

/// An `Authorization: Bearer <token>` request's [`Parts`], as the streamable-HTTP
/// transport injects them into a tool call.
fn parts_with_token(token: &str) -> http::request::Parts {
    http::Request::builder()
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(())
        .expect("build request")
        .into_parts()
        .0
}

/// A request's [`Parts`] with no `Authorization` header — the no-token path.
fn parts_without_token() -> http::request::Parts {
    http::Request::builder()
        .body(())
        .expect("build request")
        .into_parts()
        .0
}

fn create_args(id: &str) -> WorkflowCreateArgs {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "title": "MCP spike workflow",
        "workspace": { "repo": "/home/user/code/rust/lazybones" },
    }))
    .expect("deserialize workflow.create args")
}

#[tokio::test]
async fn author_token_creates_workflow_that_exists_via_store() {
    let store = store().await;
    let token = "author-secret";
    let server = server_with_author_token(store.clone(), token);

    // The Author token creates the workflow over the tool surface.
    let result = server
        .workflow_create(
            Extension(parts_with_token(token)),
            Parameters(create_args("mcp-service")),
        )
        .await
        .expect("workflow.create should succeed for an Author token");

    // The tool returns the created run as JSON.
    assert_eq!(result.0["id"], "mcp-service");
    assert_eq!(result.0["lifecycle"], "active");

    // And it now exists via the *store* — the REST surface's read boundary. The two
    // front doors share one source of truth.
    let stored = store
        .get_run("mcp-service")
        .await
        .expect("store query")
        .expect("the created run exists");
    assert_eq!(stored.id, "mcp-service");
    assert_eq!(stored.title, "MCP spike workflow");
    assert_eq!(stored.workspace.repo, "/home/user/code/rust/lazybones");
}

#[tokio::test]
async fn no_token_workflow_create_is_refused() {
    let store = store().await;
    let server = server_with_author_token(store.clone(), "author-secret");

    // No `Authorization` header ⇒ no session ⇒ a mutator refuses (authoring is gated;
    // the unauthenticated path is read-only, design §3).
    // `Json<Value>` is not `Debug`, so unwrap the error via `.err()` rather than
    // `expect_err` (which would need to format the `Ok`).
    let err = server
        .workflow_create(
            Extension(parts_without_token()),
            Parameters(create_args("unauthorized-wf")),
        )
        .await
        .err()
        .expect("workflow.create must be refused without a token");
    assert_eq!(err.message, "unauthorized");

    // And nothing was written — a refused author is a no-op.
    assert!(
        store
            .get_run("unauthorized-wf")
            .await
            .expect("store query")
            .is_none(),
        "a refused workflow.create must not create a run"
    );
}
