//! `POST /mcp/token` — minting a profile-scoped management token for an external
//! MCP client (`docs/mcp/README.md` §9 OQ1).
//!
//! Covers the auth gate (only an `Author`-bearing operator may mint), the profile
//! mapping, and — the property that matters — that the minted bearer actually
//! authenticates a subsequent guarded action exactly like any other management
//! token. No engine or git needed: a store + router is enough.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lazybones_api::{AppState, router};
use lazybones_store::{StoreEngine, StoreHandle};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

async fn app() -> Router {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "test-secret-key")
        .await
        .unwrap();
    let state = AppState::new(store, "run", "http://127.0.0.1:7878", LOOP_TOKEN);
    router(state)
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

fn mint(token: Option<&str>, body: Value) -> Request<Body> {
    let mut b = Request::post("/mcp/token").header("content-type", "application/json");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn minting_requires_a_token() {
    let app = app().await;
    let (status, _) = send(&app, mint(None, json!({ "profile": "author" }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn read_only_token_cannot_mint() {
    let app = app().await;
    // Mint a read-only token with the loop, then try to mint *with it* — it lacks
    // `Author`, so minting is refused (no privilege escalation through the surface).
    let (status, body) = send(&app, mint(Some(LOOP_TOKEN), json!({ "profile": "read_only" }))).await;
    assert_eq!(status, StatusCode::OK);
    let ro = body["token"].as_str().unwrap().to_owned();

    let (status, _) = send(&app, mint(Some(&ro), json!({ "profile": "author" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn mints_a_usable_author_token() {
    let app = app().await;
    let (status, body) = send(
        &app,
        mint(Some(LOOP_TOKEN), json!({ "profile": "author", "label": "Claude Desktop" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["profile"], "author");
    assert_eq!(body["mcp_url"], "http://127.0.0.1:7878/mcp");
    let token = body["token"].as_str().unwrap().to_owned();
    // The label is sanitised into the auditable actor segment of the token.
    assert!(
        token.contains("mcp-claude-desktop"),
        "token `{token}` should fold the sanitised label"
    );

    // The minted bearer authenticates an `Author` action just like any management
    // token: it can create a workflow.
    let req = Request::post("/workflows")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({ "id": "wf-mcp", "title": "from mcp token", "workspace": { "repo": "/tmp/x" } })
                .to_string(),
        ))
        .unwrap();
    let (status, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn author_token_cannot_start_a_workflow() {
    // The hard rule: a minted Author token authors but never starts (the create ≠
    // run line). `workflow.start` needs `Claim`, which the profile lacks.
    let app = app().await;
    let (_, body) = send(&app, mint(Some(LOOP_TOKEN), json!({ "profile": "author" }))).await;
    let token = body["token"].as_str().unwrap().to_owned();

    // Author the workflow first (so the refusal is the capability gate, not a 404).
    let create = Request::post("/workflows")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({ "id": "wf-gate", "title": "t", "workspace": { "repo": "/tmp/x" } }).to_string(),
        ))
        .unwrap();
    assert_eq!(send(&app, create).await.0, StatusCode::OK);

    let start = Request::post("/workflows/wf-gate/start")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app, start).await.0, StatusCode::FORBIDDEN);
}
