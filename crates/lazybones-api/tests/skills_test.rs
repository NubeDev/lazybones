//! The skill CRUD + generic attachment surface over REST.
//!
//! Mirrors the template story in `workflows_test.rs` but needs no engine or git:
//! a store + router is enough. Covers skill create/get/list/update/delete and the
//! `/templates/:id/attachments` routes (attach idempotent, list filters by
//! `thing_kind`, detach removes, and a missing owner 404s).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lazybones_api::{AppState, router};
use lazybones_store::{StoreEngine, StoreHandle};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

async fn store() -> StoreHandle {
    StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap()
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

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn skill_crud_roundtrip() {
    let store = store().await;
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    // Create.
    let (s, body) = send(
        &app,
        loop_req(
            "POST",
            "/skills",
            json!({
                "id": "code-review-rust",
                "title": "Rust code review",
                "description": "How to review Rust",
                "body": "Avoid unwrap in non-test code.",
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create skill: {body}");
    assert_eq!(body["id"], "code-review-rust");

    // Duplicate id → 409.
    let (s, _) = send(
        &app,
        loop_req(
            "POST",
            "/skills",
            json!({ "id": "code-review-rust", "title": "dupe", "body": "x" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "duplicate skill id");

    // Get + list (open reads).
    let (s, body) = send(&app, get("/skills/code-review-rust")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["title"], "Rust code review");

    let (s, body) = send(&app, get("/skills")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Update.
    let (s, body) = send(
        &app,
        loop_req(
            "PUT",
            "/skills/code-review-rust",
            json!({ "title": "Rust review (v2)", "body": "New rules." }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["title"], "Rust review (v2)");
    assert_eq!(body["body"], "New rules.");

    // Update unknown → 404.
    let (s, _) = send(
        &app,
        loop_req("PUT", "/skills/ghost", json!({ "title": "x", "body": "y" })),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "update unknown skill");

    // Delete (and report existence).
    let (s, body) = send(
        &app,
        loop_req("DELETE", "/skills/code-review-rust", Value::Null),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["deleted"], true);
    let (s, _) = send(&app, get("/skills/code-review-rust")).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "deleted skill is gone");
}

#[tokio::test]
async fn template_attachments_attach_list_filter_detach() {
    let store = store().await;
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    // An owner template + a skill to attach.
    let (s, _) = send(
        &app,
        loop_req(
            "POST",
            "/templates",
            json!({ "id": "open-pr", "title": "Open a PR", "spec_template": "do it" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create template");
    let (s, _) = send(
        &app,
        loop_req(
            "POST",
            "/skills",
            json!({ "id": "review", "title": "Review", "body": "x" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create skill");

    // Attach the skill — and a second, different thing-kind.
    let (s, _) = send(
        &app,
        loop_req(
            "POST",
            "/templates/open-pr/attachments",
            json!({ "thing_kind": "skill", "thing_id": "review" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "attach skill");
    send(
        &app,
        loop_req(
            "POST",
            "/templates/open-pr/attachments",
            json!({ "thing_kind": "note", "thing_id": "n-1" }),
        ),
    )
    .await;

    // Attach is idempotent — re-attaching the skill doesn't create a duplicate.
    send(
        &app,
        loop_req(
            "POST",
            "/templates/open-pr/attachments",
            json!({ "thing_kind": "skill", "thing_id": "review" }),
        ),
    )
    .await;

    // List all → 2; filter by thing_kind=skill → 1.
    let (s, body) = send(&app, get("/templates/open-pr/attachments")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        body.as_array().unwrap().len(),
        2,
        "two distinct attachments"
    );

    let (s, body) = send(&app, get("/templates/open-pr/attachments?thing_kind=skill")).await;
    assert_eq!(s, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["thing_id"], "review");

    // Detach the skill.
    let (s, body) = send(
        &app,
        loop_req(
            "DELETE",
            "/templates/open-pr/attachments/skill/review",
            Value::Null,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["deleted"], true);

    let (s, body) = send(&app, get("/templates/open-pr/attachments?thing_kind=skill")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(body.as_array().unwrap().is_empty(), "skill detached");
}

#[tokio::test]
async fn attaching_to_a_missing_template_is_404() {
    let store = store().await;
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_req(
            "POST",
            "/templates/ghost/attachments",
            json!({ "thing_kind": "skill", "thing_id": "review" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "attach to missing owner");

    let (s, _) = send(&app, get("/templates/ghost/attachments")).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "list on missing owner");
}

#[tokio::test]
async fn structured_action_skill_validates_and_persists() {
    let store = store().await;
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    // A malformed action (template references an undeclared param) → 400.
    let (s, _) = send(
        &app,
        loop_req(
            "POST",
            "/skills",
            json!({
                "id": "promote",
                "title": "Promote",
                "action": {
                    "method": "POST",
                    "path_template": "/tasks/{id}/ready",
                    "params": []
                }
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "undeclared param must 400");

    // A well-formed action persists and round-trips on read.
    let (s, body) = send(
        &app,
        loop_req(
            "POST",
            "/skills",
            json!({
                "id": "promote",
                "title": "Promote",
                "action": {
                    "method": "POST",
                    "path_template": "/tasks/{id}/ready",
                    "params": [{ "name": "id", "required": true, "description": "task id" }]
                }
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["action"]["path_template"], "/tasks/{id}/ready");

    let (_, got) = send(&app, get("/skills/promote")).await;
    assert_eq!(got["action"]["params"][0]["name"], "id");
}
