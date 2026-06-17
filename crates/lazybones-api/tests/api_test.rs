//! End-to-end REST tests over an in-memory store, driving the router directly.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lazybones_api::{AppState, router};
use lazybones_store::{SeedTask, StoreEngine, StoreHandle, sync_seeds};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

async fn app() -> Router {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test")
        .await
        .unwrap();
    sync_seeds(
        &store,
        "run",
        &[
            SeedTask {
                id: "store".into(),
                title: "store".into(),
                spec: "build the store".into(),
                deps: vec![],
                owns: vec![],
                tool: None,
            },
            SeedTask {
                id: "api".into(),
                title: "api".into(),
                spec: "build the api".into(),
                deps: vec!["store".into()],
                owns: vec![],
                tool: None,
            },
        ],
    )
    .await
    .unwrap();
    let state = AppState::new(store, "run", LOOP_TOKEN);
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

fn loop_post(path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {LOOP_TOKEN}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn full_lifecycle_over_rest() {
    let app = app().await;

    // Promote the no-dep task to ready.
    let (status, ready) = send(&app, loop_post("/tasks/promote", json!(null))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ready, json!(["store"]));

    // Claim it: mints an agent token.
    let (status, task) = send(
        &app,
        loop_post(
            "/tasks/store/claim",
            json!({
                "session": "sess-1",
                "worktree": "/wt/store",
                "branch": "lazy/store",
                "token": "agent-tok"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["status"], "running");

    // The agent heartbeats with its own scoped token.
    let hb = Request::builder()
        .method("POST")
        .uri("/tasks/store/heartbeat")
        .header("authorization", "Bearer agent-tok")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&app, hb).await;
    assert_eq!(status, StatusCode::OK);

    // Loop gates then marks done.
    let (status, _) = send(&app, loop_post("/tasks/store/gate", json!(null))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, task) = send(
        &app,
        loop_post("/tasks/store/done", json!({ "commit": "abc123" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["status"], "done");
    assert_eq!(task["commit"], "abc123");

    // Dependent task is now ready.
    let (_, ready) = send(&app, loop_post("/tasks/promote", json!(null))).await;
    assert_eq!(ready, json!(["api"]));
}

#[tokio::test]
async fn missing_token_is_unauthorized() {
    let app = app().await;
    let req = Request::builder()
        .method("POST")
        .uri("/tasks/promote")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn illegal_transition_is_conflict() {
    let app = app().await;
    // store is `pending`; done requires `gating` -> 409.
    let (status, _) = send(
        &app,
        loop_post("/tasks/store/done", json!({ "commit": "x" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn agent_cannot_act_on_another_task() {
    let app = app().await;
    // Promote + claim `store`, minting an agent token bound to `store`.
    send(&app, loop_post("/tasks/promote", json!(null))).await;
    send(
        &app,
        loop_post(
            "/tasks/store/claim",
            json!({
                "session": "s", "worktree": "w", "branch": "b", "token": "agent-tok"
            }),
        ),
    )
    .await;

    // That agent token may not heartbeat a different task.
    let req = Request::builder()
        .method("POST")
        .uri("/tasks/api/heartbeat")
        .header("authorization", "Bearer agent-tok")
        .body(Body::empty())
        .unwrap();
    let (status, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
