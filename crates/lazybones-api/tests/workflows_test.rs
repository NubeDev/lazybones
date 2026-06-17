//! The full workflow user story over REST: two reusable templates, three
//! concurrent workflows (wf-1 new + wf-2 reuse(wf-1's tree) on the SAME repo,
//! wf-3 on a DIFFERENT repo), tasks added from templates, all started, ticks
//! driven by the engine harness with a fake `hcom`, asserting every task reaches
//! `done`, wf-2's reuse task resolved wf-1's worktree, and all three progressed
//! concurrently (none starved).
//!
//! This wires the REST router and the in-process scheduler over ONE shared store
//! (the daemon's real shape): author over HTTP, drive the execution plane via
//! `lazybones_engine::harness::Engine` exactly like `tick_walk_test`.

use std::path::Path;
use std::process::Command;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use lazybones_api::{AppState, router};
use lazybones_engine::{EngineConfig, MergeMode, harness::Engine};
use lazybones_store::{Status, StoreEngine, StoreHandle};
use serde_json::{Value, json};
use tower::ServiceExt;

const LOOP_TOKEN: &str = "loop-tok";

/// Write the same fake `hcom` the scheduler integration test uses: it prints a
/// spawned name on launch, emits a DONE message on `events --wait`, and `[]` on
/// `list --json`. Returns its path.
fn write_fake_hcom(dir: &Path) -> String {
    let path = dir.join("hcom");
    let script = r#"#!/bin/sh
case "$1" in
  list)
    echo '[]'
    ;;
  events)
    echo '{"id":1,"ts":"2026-01-01T00:00:00Z","type":"message","instance":"testagent","data":{"text":"DONE","thread":"x"}}'
    ;;
  *)
    echo "Names: testagent"
    ;;
esac
exit 0
"#;
    std::fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path.to_string_lossy().into_owned()
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A git repo with one commit and a local bare remote `origin` (so push works).
/// Returns the work repo path.
fn init_repo_with_remote(root: &Path, name: &str) -> std::path::PathBuf {
    let bare = root.join(format!("{name}.git"));
    Command::new("git").args(["init", "--bare"]).arg(&bare).output().unwrap();

    let repo = root.join(name);
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("README.md"), "x").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "init"]);
    git(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&repo, &["push", "origin", "main"]);
    repo
}

fn engine_cfg(default_repo: &Path) -> EngineConfig {
    EngineConfig {
        // A workflow task's repo comes from its workspace; this global default
        // only applies to standalone tasks (none here).
        target_repo: default_repo.to_path_buf(),
        base_branch: "main".into(),
        remote: "origin".into(),
        gate: vec!["true".into()],
        concurrency: 5,
        // Real worktrees so `reuse` is exercised (false would collapse to branch).
        worktrees: true,
        worktree_root: ".lazy/wt".into(),
        branch_prefix: "lazy/".into(),
        merge: MergeMode::FastForward,
        agent_tool: "claude".into(),
        stale_after_secs: 300,
        tick_secs: 1,
    }
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

fn get(path: &str) -> Request<Body> {
    Request::builder().method("GET").uri(path).body(Body::empty()).unwrap()
}

async fn status_of(store: &StoreHandle, id: &str) -> Status {
    store.get_task(id).await.unwrap().unwrap().status
}

/// Poll a task's status up to ~10s, returning whether it reached `target`.
async fn wait_for(store: &StoreHandle, id: &str, target: Status) -> bool {
    for _ in 0..200 {
        if status_of(store, id).await == target {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn three_workflows_run_concurrently_with_reuse() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_abc = init_repo_with_remote(tmp.path(), "abc"); // wf-1 + wf-2 share this
    let repo_xyz = init_repo_with_remote(tmp.path(), "xyz"); // wf-3
    let hcom_bin = write_fake_hcom(tmp.path());

    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(store.clone(), "run", LOOP_TOKEN));

    // 1. Two reusable templates.
    for (id, title) in [("code-review", "Code review"), ("open-pr", "Open a PR")] {
        let (s, _) = send(
            &app,
            loop_post(
                "/templates",
                json!({ "id": id, "title": title, "spec_template": format!("Do the {id} work.") }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "create template {id}");
    }
    // Duplicate template id → 409.
    let (s, _) = send(
        &app,
        loop_post("/templates", json!({ "id": "open-pr", "title": "dupe", "spec_template": "x" })),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 2. Three workflows. wf-1 (new) and wf-2 (reuse) on repo abc; wf-3 on xyz.
    let mk_workflow = |id: &str, repo: &Path, mode: &str| {
        loop_post(
            "/workflows",
            json!({
                "id": id,
                "title": id,
                "workspace": { "repo": repo.to_string_lossy(), "worktree_mode": mode }
            }),
        )
    };
    assert_eq!(send(&app, mk_workflow("wf-1", &repo_abc, "new")).await.0, StatusCode::OK);
    assert_eq!(send(&app, mk_workflow("wf-2", &repo_abc, "reuse")).await.0, StatusCode::OK);
    assert_eq!(send(&app, mk_workflow("wf-3", &repo_xyz, "new")).await.0, StatusCode::OK);

    // 3. Tasks. wf-1 has a from-template task whose tree wf-2 will reuse. It
    //    overrides the workspace `new` mode to `branch` so its recorded worktree
    //    (the repo checkout) persists past `done` — `new`-mode trees are torn
    //    down on green merge, which would legitimately block a later reuse.
    let (s, _) = send(
        &app,
        loop_post(
            "/workflows/wf-1/tasks",
            json!({
                "id": "wf1-api",
                "title": "new-api",
                "from_template": "open-pr",
                "worktree_mode_override": "branch"
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // wf-2's task reuses wf-1's `wf1-api` worktree (cross-workflow reuse).
    let (s, _) = send(
        &app,
        loop_post(
            "/workflows/wf-2/tasks",
            json!({
                "id": "wf2-ui",
                "title": "new-ui",
                "from_template": "code-review",
                "reuse_from": "wf1-api"
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // wf-3: a plain task (no template) on the other repo.
    let (s, _) = send(
        &app,
        loop_post(
            "/workflows/wf-3/tasks",
            json!({ "id": "wf3-build", "title": "build", "spec": "build xyz" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // The add-task linked run_id (provenance) correctly.
    let (_, detail) = send(&app, get("/workflows/wf-1")).await;
    assert_eq!(detail["task_ids"], json!(["wf1-api"]));
    assert_eq!(detail["state"], "draft", "no task promoted yet");

    let engine = Engine::with_hcom_bin(store.clone(), engine_cfg(&repo_abc), &hcom_bin);

    // 4. Start wf-1 and wf-3 first; drive ticks so wf-1's `wf1-api` claims and
    //    records its worktree (which wf-2's reuse task depends on existing).
    assert_eq!(send(&app, loop_post("/workflows/wf-1/start", json!(null))).await.0, StatusCode::OK);
    assert_eq!(send(&app, loop_post("/workflows/wf-3/start", json!(null))).await.0, StatusCode::OK);

    engine.tick().await;
    assert!(wait_for(&store, "wf1-api", Status::Done).await, "wf1-api should finish");
    assert!(wait_for(&store, "wf3-build", Status::Done).await, "wf3-build should finish");

    // wf-1's task recorded a worktree under repo abc — the tree wf-2 will reuse.
    let wf1_worktree = store.get_task("wf1-api").await.unwrap().unwrap().worktree.unwrap();
    assert!(
        wf1_worktree.contains("abc") && Path::new(&wf1_worktree).is_dir(),
        "wf1-api worktree is a real dir under abc: {wf1_worktree}"
    );

    // 5. Now start wf-2; its reuse task resolves wf-1's worktree on claim.
    assert_eq!(send(&app, loop_post("/workflows/wf-2/start", json!(null))).await.0, StatusCode::OK);
    engine.tick().await;
    assert!(wait_for(&store, "wf2-ui", Status::Done).await, "wf2-ui should finish");

    // The reuse task resolved wf-1's worktree path exactly.
    let wf2_worktree = store.get_task("wf2-ui").await.unwrap().unwrap().worktree.unwrap();
    assert_eq!(wf2_worktree, wf1_worktree, "wf-2 reused wf-1's worktree");

    // 6. All three workflows are `done`, proving concurrent progress (none
    //    starved the others — each reached terminal state).
    for wf in ["wf-1", "wf-2", "wf-3"] {
        let (_, detail) = send(&app, get(&format!("/workflows/{wf}"))).await;
        assert_eq!(detail["state"], "done", "{wf} should be done: {detail}");
        assert_eq!(detail["done_count"], detail["task_count"]);
    }
}
