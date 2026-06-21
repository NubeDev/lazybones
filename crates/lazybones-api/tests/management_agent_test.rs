//! The Lazybones-Agent settings + chat surface over REST.
//!
//! A store + router is enough (no live hcom): covers the management-agent config
//! get/put (default when unset, catalog validation, Author-gated write) and the
//! agent-chat plumbing (open a conversation, persist the operator turn, fetch
//! history). Spawning the actual agent is the engine's job and not exercised here.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lazybones_api::{AppState, router};
use lazybones_store::{ConfirmAction, StoreEngine, StoreHandle};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

async fn store() -> StoreHandle {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    // The config validates tool/model/effort against the catalog, so seed it.
    store.seed_default_agents(&store.now()).await.unwrap();
    store
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

fn loop_req(method: &str, path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {LOOP_TOKEN}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn anon(method: &str, path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get(path: &str) -> Request<Body> {
    Request::builder().method("GET").uri(path).body(Body::empty()).unwrap()
}

fn app(store: StoreHandle) -> Router {
    router(AppState::new(store, "run", "http://127.0.0.1:0", LOOP_TOKEN))
}

#[tokio::test]
async fn config_defaults_until_written_then_roundtrips() {
    let app = app(store().await);

    // Unset → a usable default (claude/author).
    let (s, body) = send(&app, get("/settings/management-agent")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["tool"], "claude");
    assert_eq!(body["permission_profile"], "author");

    // Write a valid config.
    let (s, body) = send(
        &app,
        loop_req(
            "PUT",
            "/settings/management-agent",
            json!({
                "tool": "claude",
                "model": "claude-opus-4-8",
                "effort": "high",
                "permission_profile": "read_only",
                "session_mode": "per_turn",
                "enabled_skills": ["lazybones-add-workflow"]
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["permission_profile"], "read_only");
    assert_eq!(body["session_mode"], "per_turn");

    // Read it back.
    let (_, body) = send(&app, get("/settings/management-agent")).await;
    assert_eq!(body["effort"], "high");
    assert_eq!(body["enabled_skills"][0], "lazybones-add-workflow");
}

#[tokio::test]
async fn config_put_requires_author_and_validates_catalog() {
    let app = app(store().await);

    // No token → 401.
    let (s, _) = send(
        &app,
        anon(
            "PUT",
            "/settings/management-agent",
            json!({ "tool": "claude", "permission_profile": "author" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Unknown tool → 400.
    let (s, _) = send(
        &app,
        loop_req(
            "PUT",
            "/settings/management-agent",
            json!({ "tool": "nope", "permission_profile": "author" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Model not in the tool's catalog → 400.
    let (s, _) = send(
        &app,
        loop_req(
            "PUT",
            "/settings/management-agent",
            json!({ "tool": "claude", "model": "gpt-9", "permission_profile": "author" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_opens_a_conversation_and_persists_the_turn() {
    let app = app(store().await);

    // Posting with no conversation opens one and stores the operator turn.
    let (s, body) = send(
        &app,
        anon(
            "POST",
            "/agent/chat",
            json!({ "text": "what is the state of things?", "page_context": { "view": "dashboard" } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let conversation = body["conversation"].as_str().unwrap().to_owned();
    assert!(!conversation.is_empty());
    assert_eq!(body["message"]["role"], "user");

    // History replays the operator turn (the agent reply is async/absent here).
    let (s, hist) = send(&app, get(&format!("/agent/chat/{conversation}"))).await;
    assert_eq!(s, StatusCode::OK);
    assert!(hist.as_array().unwrap().iter().any(|m| m["role"] == "user"));

    // The conversation shows up in the list.
    let (_, convs) = send(&app, get("/agent/conversations")).await;
    assert!(convs.as_array().unwrap().iter().any(|c| c["id"] == conversation));

    // Empty text is rejected.
    let (s, _) = send(&app, anon("POST", "/agent/chat", json!({ "text": "   " }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Unknown conversation → 404.
    let (s, _) = send(&app, get("/agent/chat/does-not-exist")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn manage_profile_saves_and_confirm_requests_replay_in_history() {
    let store = store().await;
    let app = app(store.clone());

    // The manage profile is accepted and persisted.
    let (s, body) = send(
        &app,
        loop_req(
            "PUT",
            "/settings/management-agent",
            json!({ "tool": "claude", "permission_profile": "author_and_manage" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["permission_profile"], "author_and_manage");

    // A confirm request the runner would persist replays in history with its
    // exact action intact (the UI renders it as a Confirm/Cancel card).
    let conv = store.create_agent_conversation(None).await.unwrap();
    let action = ConfirmAction {
        action: "start".into(),
        method: "POST".into(),
        path: "/workflows/wf-1/start".into(),
        body: None,
    };
    store
        .append_confirm_request(&conv.id, "Start workflow wf-1?", &action, None)
        .await
        .unwrap();

    let (s, hist) = send(&app, get(&format!("/agent/chat/{}", conv.id))).await;
    assert_eq!(s, StatusCode::OK);
    let confirm = hist
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "confirm")
        .expect("confirm message present");
    assert_eq!(confirm["action"]["action"], "start");
    assert_eq!(confirm["action"]["path"], "/workflows/wf-1/start");
}

#[tokio::test]
async fn workflow_override_resolves_then_reverts() {
    let app = app(store().await);

    // Global config: author profile.
    send(
        &app,
        loop_req(
            "PUT",
            "/settings/management-agent",
            json!({ "tool": "claude", "permission_profile": "author" }),
        ),
    )
    .await;

    // With no override, the workflow resolves to the global config.
    let (s, body) = send(&app, get("/settings/management-agent/workflows/wf-1")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["permission_profile"], "author");

    // Set a workflow override (read-only); resolution now prefers it.
    let (s, _) = send(
        &app,
        loop_req(
            "PUT",
            "/settings/management-agent/workflows/wf-1",
            json!({ "tool": "claude", "permission_profile": "read_only" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, body) = send(&app, get("/settings/management-agent/workflows/wf-1")).await;
    assert_eq!(body["permission_profile"], "read_only");

    // The global config is untouched.
    let (_, g) = send(&app, get("/settings/management-agent")).await;
    assert_eq!(g["permission_profile"], "author");

    // Deleting the override reverts the workflow to global.
    let (s, body) = send(
        &app,
        loop_req("DELETE", "/settings/management-agent/workflows/wf-1", json!({})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["deleted"], true);
    let (_, body) = send(&app, get("/settings/management-agent/workflows/wf-1")).await;
    assert_eq!(body["permission_profile"], "author");
}
