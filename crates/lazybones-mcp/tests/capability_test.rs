//! Capability gating over the in-process MCP router (design §3 / §6.1).
//!
//! The hard rule the orchestration surface must enforce: the **default management
//! token (`Author`)** can freely *author* (workflows, tasks, templates, skills) but
//! **cannot drive lifecycle**. Every gated tool — `workflow.start` (`Claim`),
//! `workflow.stop`/`resume`/`restart` and `task.retry`/`auto_retry`/`cancel`
//! (`Block`) — must refuse an Author token with a `forbidden` error, and an
//! unauthenticated call must refuse with `unauthorized`. This drives the real tool
//! methods the way the streamable-HTTP transport does, against an in-memory store.

use std::sync::Arc;

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;

use lazybones_auth::{ManagementProfile, ScopedSession};
use lazybones_mcp::args::{
    IdArgs, TaskAutoRetryArgs, TaskCancelArgs, TaskRetryArgs, WorkflowRestartArgs,
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
/// (`Read + Author + Document` — no `Block`, no `Claim`).
fn author_server(store: StoreHandle, token: &str) -> McpServer {
    let session = ScopedSession::for_management("cap-test", ManagementProfile::Author);
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

fn id(id: &str) -> IdArgs {
    serde_json::from_value(serde_json::json!({ "id": id })).expect("IdArgs")
}

/// Every gated lifecycle tool refuses an **Author** token with `forbidden` — the
/// capability is the gate, exactly as the REST routes' `session.require` is. We
/// hit each one and assert the refusal message names the missing grant.
#[tokio::test]
async fn gated_lifecycle_tools_forbid_author_token() {
    let store = store().await;
    let token = "author-secret";
    let server = author_server(store, token);

    // `start` needs `Claim`; the rest need `Block`. The Author profile holds
    // neither, so every one must be `forbidden` (→ 403), not executed.
    macro_rules! forbidden {
        ($call:expr, $missing:expr) => {{
            let err = $call.await.err().expect("gated tool must refuse Author token");
            // The wire message is the auth layer's `missing capability: <name>`,
            // naming the specific grant the Author profile lacks.
            assert_eq!(
                err.message,
                format!("missing capability: {}", $missing),
                "unexpected refusal message for a tool missing `{}`",
                $missing
            );
        }};
    }

    forbidden!(
        server.workflow_start(Extension(parts_with_token(token)), Parameters(id("wf"))),
        "claim"
    );
    forbidden!(
        server.workflow_stop(Extension(parts_with_token(token)), Parameters(id("wf"))),
        "block"
    );
    forbidden!(
        server.workflow_resume(Extension(parts_with_token(token)), Parameters(id("wf"))),
        "block"
    );
    forbidden!(
        server.workflow_restart(
            Extension(parts_with_token(token)),
            Parameters(restart("wf"))
        ),
        "block"
    );
    forbidden!(
        server.task_retry(Extension(parts_with_token(token)), Parameters(retry("t"))),
        "block"
    );
    forbidden!(
        server.task_auto_retry(Extension(parts_with_token(token)), Parameters(auto_retry("t"))),
        "block"
    );
    forbidden!(
        server.task_cancel(Extension(parts_with_token(token)), Parameters(cancel("t"))),
        "block"
    );
}

fn restart(id: &str) -> WorkflowRestartArgs {
    serde_json::from_value(serde_json::json!({ "id": id })).expect("WorkflowRestartArgs")
}
fn retry(id: &str) -> TaskRetryArgs {
    serde_json::from_value(serde_json::json!({ "id": id })).expect("TaskRetryArgs")
}
fn auto_retry(id: &str) -> TaskAutoRetryArgs {
    serde_json::from_value(serde_json::json!({ "id": id })).expect("TaskAutoRetryArgs")
}
fn cancel(id: &str) -> TaskCancelArgs {
    serde_json::from_value(serde_json::json!({ "id": id })).expect("TaskCancelArgs")
}

/// An unauthenticated lifecycle call is refused as `unauthorized` (a mutator never
/// falls through to the read path) — and it never reaches the store.
#[tokio::test]
async fn unauthenticated_lifecycle_call_is_unauthorized() {
    let store = store().await;
    let server = author_server(store, "author-secret");

    let err = server
        .workflow_start(Extension(parts_without_token()), Parameters(id("wf")))
        .await
        .err()
        .expect("an unauthenticated lifecycle call must refuse");
    assert_eq!(err.message, "unauthorized");
}
