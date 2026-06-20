//! End-to-end REST tests over an in-memory store, driving the router directly.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tokio_stream::StreamExt;
use lazybones_api::{AppState, router};
use lazybones_store::{SeedTask, StoreEngine, StoreHandle, sync_seeds};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

async fn app() -> Router {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "test-secret-key")
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
                reuse_from: None,
            },
            SeedTask {
                id: "api".into(),
                title: "api".into(),
                spec: "build the api".into(),
                deps: vec!["store".into()],
                owns: vec![],
                tool: None,
                reuse_from: None,
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
    loop_req("POST", path, Some(body))
}

/// A loop-authenticated request with an arbitrary method and optional JSON body.
fn loop_req(method: &str, path: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {LOOP_TOKEN}"));
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    builder.body(body).unwrap()
}

/// An unauthenticated GET (reads are open).
fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn fs_list_browses_dirs_and_flags_repos() {
    let app = app().await;

    // A temp dir holding a plain subdir, a git repo (has `.git`), and a dotdir.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("plain")).unwrap();
    std::fs::create_dir(tmp.path().join("repo")).unwrap();
    std::fs::create_dir(tmp.path().join("repo/.git")).unwrap();
    std::fs::create_dir(tmp.path().join(".hidden")).unwrap();

    let uri = format!("/fs/list?path={}", tmp.path().display());
    let (status, body) = send(&app, get(&uri)).await;
    assert_eq!(status, StatusCode::OK);

    let names: Vec<&str> = body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    // Dotdirs are hidden; plain + repo are listed and sorted.
    assert_eq!(names, vec!["plain", "repo"]);

    let repo = &body["entries"][1];
    assert_eq!(repo["name"], "repo");
    assert_eq!(repo["is_repo"], true);
    assert_eq!(body["entries"][0]["is_repo"], false);
    assert!(body["parent"].is_string());
}

#[tokio::test]
async fn gh_worktrees_lists_main_and_extra() {
    let app = app().await;

    // A real temp repo with one extra worktree on its own branch.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let run_git = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap()
    };
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@t"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "root"],
    ] {
        run_git(args);
    }
    let wt = dir.join("wt-extra");
    run_git(&["worktree", "add", "-q", "-b", "feat/wt", wt.to_str().unwrap()]);

    let uri = format!("/gh/worktrees?dir={}", urlencode(&dir.to_string_lossy()));
    let (status, body) = send(&app, loop_req("GET", &uri, None)).await;
    assert_eq!(status, StatusCode::OK);
    let trees = body.as_array().unwrap();
    assert_eq!(trees.len(), 2);
    assert_eq!(trees[0]["is_main"], json!(true));
    assert!(
        trees
            .iter()
            .any(|w| w["branch"] == json!("feat/wt") && w["is_main"] == json!(false))
    );
}

#[tokio::test]
async fn gh_local_branches_works_without_remote() {
    let app = app().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let run_git = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap()
    };
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "t@t"],
        &["config", "user.name", "t"],
        &["commit", "--allow-empty", "-q", "-m", "root"],
        &["branch", "feat/x"],
    ] {
        run_git(args);
    }

    let uri = format!("/gh/local-branches?dir={}", urlencode(&dir.to_string_lossy()));
    let (status, body) = send(&app, loop_req("GET", &uri, None)).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"feat/x"));
    // No remote → upstream null, ahead/behind 0 — and no error.
    let feat = body
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == json!("feat/x"))
        .unwrap();
    assert_eq!(feat["upstream"], Value::Null);
    assert_eq!(feat["ahead"], json!(0));
}

/// Minimal percent-encoding for a filesystem path used in a query string.
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'/' => "%2F".to_string(),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[tokio::test]
async fn gh_auth_probe_is_unguarded_and_returns_a_verdict() {
    let app = app().await;
    // No token, no network assumptions: the probe always answers 200 with a
    // boolean (true if `gh` is logged in on this host, false otherwise).
    let (status, body) = send(&app, get("/gh/auth")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["authenticated"].is_boolean());
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
async fn stream_pushes_a_transition_event() {
    let app = app().await;

    // Open the SSE stream first so the subscription exists before the transition.
    let stream_res = app
        .clone()
        .oneshot(get("/stream"))
        .await
        .unwrap();
    assert_eq!(stream_res.status(), StatusCode::OK);
    assert_eq!(
        stream_res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream"),
    );
    let mut body = stream_res.into_body().into_data_stream();

    // Drive a transition on the shared state; it must surface on the stream.
    let (status, _) = send(&app, loop_post("/tasks/promote", json!(null))).await;
    assert_eq!(status, StatusCode::OK);

    // Read SSE frames until the `transition` payload arrives (skipping keep-alives).
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let chunk = body
                .next()
                .await
                .expect("stream yields a frame")
                .expect("frame is ok");
            let text = String::from_utf8(chunk.to_vec()).unwrap();
            if text.contains("event:transition") || text.contains("event: transition") {
                return text;
            }
        }
    })
    .await
    .expect("a transition frame within the timeout");

    assert!(frame.contains("\"task\":\"store\""), "frame: {frame}");
    assert!(frame.contains("\"to\":\"ready\""), "frame: {frame}");
}

#[tokio::test]
async fn stream_pushes_an_agent_activity_message() {
    let app = app().await;

    // Promote + claim `store` so an agent token exists, bound to the task.
    send(&app, loop_post("/tasks/promote", json!(null))).await;
    send(
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

    // Open the stream before the agent reports.
    let stream_res = app.clone().oneshot(get("/stream")).await.unwrap();
    assert_eq!(stream_res.status(), StatusCode::OK);
    let mut body = stream_res.into_body().into_data_stream();

    // The agent reports progress on its own task with its scoped token.
    let activity = Request::builder()
        .method("POST")
        .uri("/tasks/store/activity")
        .header("authorization", "Bearer agent-tok")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "message": "running cargo test" }).to_string()))
        .unwrap();
    let (status, _) = send(&app, activity).await;
    assert_eq!(status, StatusCode::OK);

    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let chunk = body.next().await.unwrap().unwrap();
            let text = String::from_utf8(chunk.to_vec()).unwrap();
            if text.contains("activity") {
                return text;
            }
        }
    })
    .await
    .expect("an activity frame within the timeout");

    assert!(frame.contains("\"task\":\"store\""), "frame: {frame}");
    assert!(
        frame.contains("\"message\":\"running cargo test\""),
        "frame: {frame}"
    );
}

#[tokio::test]
async fn agent_cannot_report_activity_on_another_task() {
    let app = app().await;
    // Claim `store` (agent token bound to `store`), then try to report on `api`.
    send(&app, loop_post("/tasks/promote", json!(null))).await;
    send(
        &app,
        loop_post(
            "/tasks/store/claim",
            json!({
                "session": "s", "worktree": "/wt/store",
                "branch": "lazy/store", "token": "agent-tok"
            }),
        ),
    )
    .await;
    let req = Request::builder()
        .method("POST")
        .uri("/tasks/api/activity")
        .header("authorization", "Bearer agent-tok")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "message": "sneaky" }).to_string()))
        .unwrap();
    let (status, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn ready_promotes_one_task() {
    let app = app().await;
    // `store` has no deps, so a single-task promote is legal pending -> ready.
    let (status, task) = send(&app, loop_post("/tasks/store/ready", json!(null))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["status"], "ready");
    // `api` still depends on `store` (not done) — but the state machine itself
    // permits pending -> ready, so the route promotes it when asked directly.
    // A second promote of an already-ready task is an illegal transition -> 409.
    let (status, _) = send(&app, loop_post("/tasks/store/ready", json!(null))).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn hcom_log_routes_are_wired_and_read_the_durable_log() {
    let app = app().await;

    // The run's hcom log is empty (nothing tailed yet) but the route is wired and
    // reads the durable table: 200 with an empty array.
    let (status, body) = send(&app, get("/runs/run/hcom")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));

    // `GET /tasks/:id/hcom` resolves the task's run, then filters to it: 200, [].
    let (status, body) = send(&app, get("/tasks/store/hcom")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));

    // An unknown task is a 404, not an empty log.
    let (status, _) = send(&app, get("/tasks/nope/hcom")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Filter params parse and apply without error.
    let (status, body) = send(&app, get("/runs/run/hcom?kind=message&after=0&limit=10")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
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
async fn cancel_blocks_a_running_task() {
    let app = app().await;
    // Promote + claim `store` so it is `running`.
    send(&app, loop_post("/tasks/promote", json!(null))).await;
    let (status, _) = send(
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

    // Cancel: the hcom kill is best-effort (no real agent here), so the task
    // still lands in `blocked` with the supplied reason.
    let (status, task) = send(
        &app,
        loop_post("/tasks/store/cancel", json!({ "reason": "operator stop" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["status"], "blocked");
    assert_eq!(task["reason"], "operator stop");
}

#[tokio::test]
async fn cancel_defaults_reason_when_omitted() {
    let app = app().await;
    send(&app, loop_post("/tasks/promote", json!(null))).await;
    send(
        &app,
        loop_post(
            "/tasks/store/claim",
            json!({
                "session": "s",
                "worktree": "/wt",
                "branch": "b",
                "token": "agent-tok"
            }),
        ),
    )
    .await;

    let (status, task) = send(&app, loop_post("/tasks/store/cancel", json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["status"], "blocked");
    assert_eq!(task["reason"], "cancelled by operator");
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

#[tokio::test]
async fn author_create_update_delete_over_rest() {
    let app = app().await;

    // Author a brand-new task that depends on the seeded `api` task.
    let (status, task) = send(
        &app,
        loop_post(
            "/tasks",
            json!({
                "id": "ui",
                "title": "build the ui",
                "spec": "react front-end",
                "deps": ["api"],
                "owns": ["ui/**"],
                "tool": "claude"
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["status"], "pending");
    assert_eq!(task["deps"], json!(["api"]));
    assert_eq!(task["tool"], "claude");

    // The dependency edge is real: `ui` is not ready until `api` is done.
    let (_, ready) = send(&app, loop_post("/tasks/promote", json!(null))).await;
    assert_eq!(ready, json!(["store"]), "only the no-dep task is ready");

    // Edit it: change the spec and drop the dependency entirely.
    let (status, task) = send(
        &app,
        loop_req(
            "PATCH",
            "/tasks/ui",
            Some(json!({
                "title": "build the ui",
                "spec": "react + vite front-end",
                "deps": [],
                "owns": ["ui/**"],
                "tool": "claude"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["spec"], "react + vite front-end");
    assert_eq!(task["deps"], json!([]));

    // With its dep dropped, `ui` now promotes to ready alongside `store`.
    let (_, ready) = send(&app, loop_post("/tasks/promote", json!(null))).await;
    let ready: Vec<String> = serde_json::from_value(ready).unwrap();
    assert!(ready.contains(&"ui".to_string()), "ui ready after dep dropped");

    // Delete it; the read then 404s.
    let (status, body) = send(&app, loop_req("DELETE", "/tasks/ui", None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], json!(true));
    let (status, _) = send(&app, get("/tasks/ui")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_duplicate_id_is_conflict() {
    let app = app().await;
    // `store` already exists from the seed.
    let (status, _) = send(
        &app,
        loop_post(
            "/tasks",
            json!({ "id": "store", "title": "dupe", "spec": "x" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn update_missing_task_is_not_found() {
    let app = app().await;
    let (status, _) = send(
        &app,
        loop_req(
            "PATCH",
            "/tasks/ghost",
            Some(json!({ "title": "t", "spec": "s" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn agent_cannot_author_tasks() {
    let app = app().await;
    // Promote + claim `store`, minting an agent token (no `Author` grant).
    send(&app, loop_post("/tasks/promote", json!(null))).await;
    send(
        &app,
        loop_post(
            "/tasks/store/claim",
            json!({ "session": "s", "worktree": "w", "branch": "b", "token": "agent-tok" }),
        ),
    )
    .await;

    let req = Request::builder()
        .method("POST")
        .uri("/tasks")
        .header("authorization", "Bearer agent-tok")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "id": "sneaky", "title": "t", "spec": "s" }).to_string(),
        ))
        .unwrap();
    let (status, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
