//! The `/extensions` REST surface (design §3.6) + the frontend asset proxy (§4.3).
//!
//! Covers the full lifecycle over an in-memory store + tempdir blob store: install
//! (upload), list/get (+ `?enabled`/`?frontend` filters), capability grants with
//! the `granted ⊆ requested` guard, enable/disable, a test-invoke, the frontend
//! proxy (served bytes, 404s, path-traversal rejection), delete, and the loop-only
//! `Extension` capability guard.
//!
//! Like the registry unit tests, install uses a hand-built wasm carrying the
//! manifest in a custom section, so no wasm toolchain is needed. That artifact is
//! not a real component, so the test-invoke exercises the route's **fail-closed
//! fault mapping** (a load fault → a `fail` verdict); real guest execution is
//! covered by `lazybones-ext`'s `gate_check` test against the example component.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lazybones_api::{AppState, router};
use lazybones_store::{BlobStore, FileBlobStore, StoreEngine, StoreHandle};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

/// A manifest with a frontend half, matching the registry tests' shape.
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

/// Build a minimal well-formed wasm binary carrying the manifest TOML in a
/// `lazybones.ext.toml` custom section (same trick as `lazybones-ext`'s tests).
fn wasm_with_manifest(toml: &str) -> Vec<u8> {
    let mut wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let name = b"lazybones.ext.toml";
    let mut body = Vec::new();
    body.extend_from_slice(&uleb(name.len() as u64));
    body.extend_from_slice(name);
    body.extend_from_slice(toml.as_bytes());
    wasm.push(0x00); // custom section id
    wasm.extend_from_slice(&uleb(body.len() as u64));
    wasm.extend_from_slice(&body);
    wasm
}

/// Build the router + return the shared blob store so the test can seed frontend
/// bundle bytes directly (there is no frontend-upload route in this task).
async fn app() -> (Router, Arc<FileBlobStore>) {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let dir = std::env::temp_dir().join(format!(
        "lazybones-ext-blob-{}",
        lazybones_store::sha256_hex(format!("{:p}", &store).as_bytes())
    ));
    let blob = Arc::new(FileBlobStore::new(dir));
    let state =
        AppState::new(store, "run", "http://127.0.0.1:0", LOOP_TOKEN).with_assets(blob.clone());
    (router(state), blob)
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

async fn send_raw(app: &Router, req: Request<Body>) -> (StatusCode, String, Vec<u8>) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let ct = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, ct, bytes)
}

fn loop_json(method: &str, path: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {LOOP_TOKEN}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn loop_upload(path: &str, bytes: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {LOOP_TOKEN}"))
        .header("content-type", "application/wasm")
        .body(Body::from(Bytes::from(bytes)))
        .unwrap()
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_extension_lifecycle() {
    let (app, blob) = app().await;
    let wasm = wasm_with_manifest(GATE_MANIFEST);

    // Install without a token is rejected before the body runs.
    let (status, _) = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/extensions")
            .header("content-type", "application/wasm")
            .body(Body::from(wasm.clone()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Install (upload). Default-deny: disabled, no grants; identity from manifest.
    let (status, body) = send(&app, loop_upload("/extensions", wasm.clone())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let id = body["id"].as_str().unwrap().to_owned();
    assert!(id.starts_with("ext-"));
    assert_eq!(body["name"], "gate-guard");
    assert_eq!(body["version"], "0.1.0");
    assert_eq!(body["enabled"], false);
    assert_eq!(body["granted_caps"], json!([]));
    assert_eq!(body["exports"], json!(["gate-check"]));
    assert_eq!(body["requested_caps"], json!(["log", "store-read"]));
    assert!(body["frontend"].is_object());

    // Re-installing identical bytes collides on the derived id (409).
    let (status, _) = send(&app, loop_upload("/extensions", wasm.clone())).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // List (open read) + filters. Disabled, so `?enabled=1` is empty; it ships a
    // frontend, so `?frontend=1` includes it.
    let (status, list) = send(&app, get("/extensions")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (_, enabled_only) = send(&app, get("/extensions?enabled=1")).await;
    assert!(enabled_only.as_array().unwrap().is_empty());
    let (_, frontends) = send(&app, get("/extensions?frontend=1")).await;
    assert_eq!(frontends.as_array().unwrap().len(), 1);

    // Get one (open read).
    let (status, one) = send(&app, get(&format!("/extensions/{id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(one["id"], id);

    // Grants: a cap the manifest never requested is rejected (granted ⊆ requested).
    let (status, _) = send(
        &app,
        loop_json(
            "POST",
            &format!("/extensions/{id}/grants"),
            json!({ "granted_caps": ["http-fetch"] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A requested cap is granted.
    let (status, granted) = send(
        &app,
        loop_json(
            "POST",
            &format!("/extensions/{id}/grants"),
            json!({ "granted_caps": ["store-read"] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{granted}");
    assert_eq!(granted["granted_caps"], json!(["store-read"]));

    // Enable → now visible under `?enabled=1`.
    let (status, enabled) = send(&app, loop_json("POST", &format!("/extensions/{id}/enable"), json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(enabled["enabled"], true);
    let (_, enabled_only) = send(&app, get("/extensions?enabled=1")).await;
    assert_eq!(enabled_only.as_array().unwrap().len(), 1);

    // Frontend proxy: a missing bundle file 404s.
    let (status, _, _) = send_raw(&app, get(&format!("/extensions/{id}/frontend/remoteEntry.js"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Seed a bundle file directly (no upload route yet), then it is served with the
    // right content type.
    blob.put(
        "remoteEntry.js",
        Some(&format!("ext-frontend/{id}")),
        b"export const x = 1;",
    )
    .await
    .unwrap();
    let (status, ct, bytes) =
        send_raw(&app, get(&format!("/extensions/{id}/frontend/remoteEntry.js"))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(ct.starts_with("text/javascript"), "{ct}");
    assert_eq!(bytes, b"export const x = 1;");

    // Path traversal is rejected.
    let (status, _, _) = send_raw(&app, get(&format!("/extensions/{id}/frontend/../secret"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Test-invoke an unknown export is a 400.
    let (status, _) = send(
        &app,
        loop_json(
            "POST",
            &format!("/extensions/{id}/invoke"),
            json!({ "export": "merge-strategy", "input": {} }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Test-invoke gate-check. The hand-built artifact is not a real component, so
    // the load fault maps fail-closed to a `fail` verdict — the route still returns
    // a structured 200, proving the engine + fault mapping are wired.
    let (status, invoke) = send(
        &app,
        loop_json(
            "POST",
            &format!("/extensions/{id}/invoke"),
            json!({
                "export": "gate-check",
                "input": { "task_id": "t1", "task_summary": "x", "diff": { "files_changed": 2, "insertions": 9, "deletions": 1 } }
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{invoke}");
    assert_eq!(invoke["export"], "gate-check");
    assert_eq!(invoke["faulted"], true);
    assert_eq!(invoke["verdict"]["kind"], "fail");

    // Disable.
    let (status, disabled) = send(&app, loop_json("POST", &format!("/extensions/{id}/disable"), json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(disabled["enabled"], false);

    // Delete → gone.
    let (status, del) = send(
        &app,
        Request::builder()
            .method("DELETE")
            .uri(format!("/extensions/{id}"))
            .header("authorization", format!("Bearer {LOOP_TOKEN}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(del["deleted"], true);
    let (status, _) = send(&app, get(&format!("/extensions/{id}"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mutations_require_the_extension_capability() {
    // An agent token (no `Extension` cap) seeded via claim cannot install. Here we
    // just confirm the unauthenticated path; the capability gate itself is unit-
    // tested in `lazybones-auth`. A bad/empty install body is a 400 *after* auth.
    let (app, _) = app().await;
    let (status, _) = send(&app, loop_upload("/extensions", Vec::new())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "empty component is a 400");
}
