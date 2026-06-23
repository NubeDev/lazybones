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
    # A launch: mimic a real agent editing its worktree so the engine's auto-commit
    # has work to commit (a no-op task is now correctly blocked). Parse `--dir`.
    dir=""
    while [ "$#" -gt 0 ]; do
      if [ "$1" = "--dir" ]; then dir="$2"; break; fi
      shift
    done
    if [ -n "$dir" ]; then echo "agent work $$" >> "$dir/agent-work.txt"; fi
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
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A git repo with one commit and a local bare remote `origin` (so push works).
/// Returns the work repo path.
fn init_repo_with_remote(root: &Path, name: &str) -> std::path::PathBuf {
    let bare = root.join(format!("{name}.git"));
    Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .output()
        .unwrap();

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
        agent_model: None,
        agent_effort: None,
        permission_flags: std::collections::HashMap::new(),
        auto_trust_agent_folder: true,
        stale_after_secs: 300,
        tick_secs: 1,
        issue_sync_every_n_ticks: 0,
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
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
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
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

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
        loop_post(
            "/templates",
            json!({ "id": "open-pr", "title": "dupe", "spec_template": "x" }),
        ),
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
    assert_eq!(
        send(&app, mk_workflow("wf-1", &repo_abc, "new")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send(&app, mk_workflow("wf-2", &repo_abc, "reuse")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        send(&app, mk_workflow("wf-3", &repo_xyz, "new")).await.0,
        StatusCode::OK
    );

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
    assert_eq!(
        send(&app, loop_post("/workflows/wf-1/start", json!(null)))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        send(&app, loop_post("/workflows/wf-3/start", json!(null)))
            .await
            .0,
        StatusCode::OK
    );

    engine.tick().await;
    assert!(
        wait_for(&store, "wf1-api", Status::Done).await,
        "wf1-api should finish"
    );
    assert!(
        wait_for(&store, "wf3-build", Status::Done).await,
        "wf3-build should finish"
    );

    // wf-1's task recorded a worktree under repo abc — the tree wf-2 will reuse.
    let wf1_worktree = store
        .get_task("wf1-api")
        .await
        .unwrap()
        .unwrap()
        .worktree
        .unwrap();
    assert!(
        wf1_worktree.contains("abc") && Path::new(&wf1_worktree).is_dir(),
        "wf1-api worktree is a real dir under abc: {wf1_worktree}"
    );

    // 5. Now start wf-2; its reuse task resolves wf-1's worktree on claim.
    assert_eq!(
        send(&app, loop_post("/workflows/wf-2/start", json!(null)))
            .await
            .0,
        StatusCode::OK
    );
    engine.tick().await;
    assert!(
        wait_for(&store, "wf2-ui", Status::Done).await,
        "wf2-ui should finish"
    );

    // The reuse task resolved wf-1's worktree path exactly.
    let wf2_worktree = store
        .get_task("wf2-ui")
        .await
        .unwrap()
        .unwrap()
        .worktree
        .unwrap();
    assert_eq!(wf2_worktree, wf1_worktree, "wf-2 reused wf-1's worktree");

    // 6. All three workflows are `done`, proving concurrent progress (none
    //    starved the others — each reached terminal state).
    for wf in ["wf-1", "wf-2", "wf-3"] {
        let (_, detail) = send(&app, get(&format!("/workflows/{wf}"))).await;
        assert_eq!(detail["state"], "done", "{wf} should be done: {detail}");
        assert_eq!(detail["done_count"], detail["task_count"]);
    }
}

/// A `reuse`-mode task whose `reuse_from` points at a task that has never been
/// claimed (so it has no stored `worktree`) must **block with a specific reason**
/// when the scheduler tries to claim it — not panic, not silently fall back. This
/// is the cross-workflow-reuse failure mode the docs require be guarded
/// (workflows-scope.md §`reuse_from`).
#[tokio::test]
async fn reuse_from_missing_target_blocks_with_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = init_repo_with_remote(tmp.path(), "abc");
    let hcom_bin = write_fake_hcom(tmp.path());

    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    // One workflow, reuse-by-default, with a single task pointing `reuse_from` at
    // a task id that does not exist anywhere — so it can never have a worktree.
    let (s, _) = send(
        &app,
        loop_post(
            "/workflows",
            json!({
                "id": "wf-r",
                "title": "wf-r",
                "workspace": { "repo": repo.to_string_lossy(), "worktree_mode": "reuse" }
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let (s, _) = send(
        &app,
        loop_post(
            "/workflows/wf-r/tasks",
            json!({
                "id": "wfr-ui",
                "title": "needs a tree it can't get",
                "spec": "x",
                "reuse_from": "ghost-task"
            }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let engine = Engine::with_hcom_bin(store.clone(), engine_cfg(&repo), &hcom_bin);
    assert_eq!(
        send(&app, loop_post("/workflows/wf-r/start", json!(null)))
            .await
            .0,
        StatusCode::OK
    );

    engine.tick().await;
    assert!(
        wait_for(&store, "wfr-ui", Status::Blocked).await,
        "task with a missing reuse_from target must block, not proceed"
    );

    // The block carries a clear, specific reason naming the missing source task —
    // never a panic or a silent fall-through to a fresh tree.
    let task = store.get_task("wfr-ui").await.unwrap().unwrap();
    let reason = task.reason.unwrap_or_default();
    assert!(
        reason.contains("reuse_from") && reason.contains("ghost-task"),
        "block reason should name the missing reuse_from target: {reason:?}"
    );

    // The workflow surfaces as needs-attention (a blocked task), not done.
    let (_, detail) = send(&app, get("/workflows/wf-r")).await;
    assert_eq!(
        detail["state"], "needs-attention",
        "blocked task → needs-attention"
    );
}

/// `GET /workflows/:id/tasks` is scoped to the workflow's `run_id`: it returns
/// only that workflow's tasks, never a sibling workflow's or a standalone task —
/// so a workflow view can't render a foreign task. Unknown workflow → 404.
#[tokio::test]
async fn workflow_tasks_endpoint_is_scoped_to_its_run_id() {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let mk_workflow = |id: &str| {
        loop_post(
            "/workflows",
            json!({ "id": id, "title": id, "workspace": { "repo": "/tmp/repo" } }),
        )
    };
    assert_eq!(send(&app, mk_workflow("wf-a")).await.0, StatusCode::OK);
    assert_eq!(send(&app, mk_workflow("wf-b")).await.0, StatusCode::OK);

    // Two tasks in wf-a, one in wf-b, and a standalone task (no workflow).
    for (wf, tid) in [("wf-a", "a1"), ("wf-a", "a2"), ("wf-b", "b1")] {
        let (s, _) = send(
            &app,
            loop_post(
                &format!("/workflows/{wf}/tasks"),
                json!({ "id": tid, "title": tid, "spec": "x" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "add {tid} to {wf}");
    }
    let (s, _) = send(
        &app,
        loop_post(
            "/tasks",
            json!({ "id": "loose", "title": "loose", "spec": "x" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "create standalone task");

    // wf-a's endpoint returns exactly its two tasks — not b1, not the standalone.
    let (s, body) = send(&app, get("/workflows/wf-a/tasks")).await;
    assert_eq!(s, StatusCode::OK);
    let mut ids: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["a1", "a2"], "wf-a sees only its own tasks");
    for t in body.as_array().unwrap() {
        assert_eq!(t["run_id"], "wf-a", "every returned task is linked to wf-a");
    }

    // wf-b sees only b1.
    let (_, body) = send(&app, get("/workflows/wf-b/tasks")).await;
    let ids: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["b1"], "wf-b sees only its own task");

    // Unknown workflow → 404, not an empty list masquerading as "no tasks".
    let (s, _) = send(&app, get("/workflows/nope/tasks")).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "unknown workflow 404s");
}

/// `POST /tasks/:id/retry` revives ONE blocked task back to `pending` (so the
/// next tick re-promotes it) and refuses a task that isn't actually stuck — a
/// `done` task is finished work, `409`. Unknown id → `404`.
#[tokio::test]
async fn retry_revives_a_blocked_task_and_rejects_a_done_one() {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_post(
            "/workflows",
            json!({ "id": "wf", "title": "wf", "workspace": { "repo": "/tmp/repo" } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // A blocked task and a done task in the same workflow.
    for tid in ["stuck", "finished"] {
        let (s, _) = send(
            &app,
            loop_post(
                "/workflows/wf/tasks",
                json!({ "id": tid, "title": tid, "spec": "x" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    // Block `stuck`, and drive `finished` to done directly in the store.
    let (s, _) = send(
        &app,
        loop_post("/tasks/stuck/block", json!({ "reason": "boom" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(status_of(&store, "stuck").await, Status::Blocked);
    drive_to_done(&store, "finished").await;

    // Retry the blocked task → back to pending, reason cleared.
    let (s, body) = send(&app, loop_post("/tasks/stuck/retry", json!(null))).await;
    assert_eq!(s, StatusCode::OK, "retry blocked → 200: {body}");
    assert_eq!(body["status"], "pending");
    assert_eq!(body["reason"], Value::Null, "block reason cleared on reset");
    assert_eq!(status_of(&store, "stuck").await, Status::Pending);

    // Retry a done task → rejected; it stays done.
    let (s, _) = send(&app, loop_post("/tasks/finished/retry", json!(null))).await;
    assert_eq!(s, StatusCode::CONFLICT, "retry of a done task is rejected");
    assert_eq!(status_of(&store, "finished").await, Status::Done);

    // Unknown task → 404.
    let (s, _) = send(&app, loop_post("/tasks/ghost/retry", json!(null))).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "unknown task 404s");
}

/// A *guided* retry (`strategy` set) revives the task in place — `blocked ->
/// ready`, worktree KEPT — and folds the strategy's guidance into the task's
/// conversation so the re-spawn prompt sees it. Unlike a clean retry it does not
/// reset to pending or clear the worktree.
#[tokio::test]
async fn guided_retry_revives_in_place_with_guidance() {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_post(
            "/workflows",
            json!({ "id": "wf", "title": "wf", "workspace": { "repo": "/tmp/repo" } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = send(
        &app,
        loop_post(
            "/workflows/wf/tasks",
            json!({ "id": "stuck", "title": "stuck", "spec": "x" }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Drive it into `blocked` carrying a worktree + a reason (the gate-red shape).
    drive_to_blocked(&store, "stuck", "gate failed: 3 tests red").await;
    let before = store.get_task("stuck").await.unwrap().unwrap();
    assert_eq!(
        before.worktree.as_deref(),
        Some("/wt"),
        "has a worktree to keep"
    );

    // Guided retry with the long-term strategy.
    let (s, body) = send(
        &app,
        loop_post("/tasks/stuck/retry", json!({ "strategy": "long_term" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "guided retry → 200: {body}");
    // Revived in place: ready (not pending), worktree kept.
    assert_eq!(body["status"], "ready", "guided retry revives, not resets");
    assert_eq!(body["worktree"], "/wt", "worktree kept for the re-spawn");

    // The guidance was written to the conversation (oldest first), naming the
    // prior reason so the re-spawned agent knows what to fix.
    let (_, chat) = send(&app, get("/tasks/stuck/chat")).await;
    let msgs = chat.as_array().unwrap();
    assert_eq!(msgs.len(), 1, "one guidance message: {chat}");
    assert_eq!(msgs[0]["role"], "user");
    let text = msgs[0]["text"].as_str().unwrap();
    assert!(
        text.contains("gate failed: 3 tests red"),
        "names the reason: {text}"
    );
    assert!(text.contains("root cause"), "long-term guidance: {text}");
}

/// `PUT /tasks/:id/auto-retry` sets and clears a task's hands-off retry policy.
#[tokio::test]
async fn auto_retry_policy_is_settable_and_clearable() {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_post("/tasks", json!({ "id": "t", "title": "t", "spec": "x" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Default: auto-retry off, cap at the default 2.
    let t = store.get_task("t").await.unwrap().unwrap();
    assert_eq!(t.auto_retry, None);
    assert_eq!(t.max_retries, 2);

    // Turn it on (quick, cap 3).
    let (s, body) = loop_put(
        &app,
        "/tasks/t/auto-retry",
        json!({ "strategy": "quick", "max_retries": 3 }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["auto_retry"], "quick");
    assert_eq!(body["max_retries"], 3);

    // Clear it (strategy null) — cap unchanged.
    let (s, body) = loop_put(&app, "/tasks/t/auto-retry", json!({ "strategy": null })).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["auto_retry"], Value::Null);
    assert_eq!(body["max_retries"], 3, "cap left unchanged when omitted");

    // Unknown task → 404.
    let (s, _) = loop_put(
        &app,
        "/tasks/ghost/auto-retry",
        json!({ "strategy": "quick" }),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

/// `POST /workflows/:id/resume` resets ONLY the workflow's blocked tasks →
/// `pending`, leaving done/running/pending alone (continue from where it broke).
/// Unknown workflow → `404`.
#[tokio::test]
async fn resume_resets_only_blocked_tasks() {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_post(
            "/workflows",
            json!({ "id": "wf", "title": "wf", "workspace": { "repo": "/tmp/repo" } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Four tasks landing in four different states.
    for tid in ["blocked-1", "blocked-2", "done-1", "pending-1"] {
        let (s, _) = send(
            &app,
            loop_post(
                "/workflows/wf/tasks",
                json!({ "id": tid, "title": tid, "spec": "x" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    for tid in ["blocked-1", "blocked-2"] {
        let (s, _) = send(
            &app,
            loop_post(&format!("/tasks/{tid}/block"), json!({ "reason": "boom" })),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    drive_to_done(&store, "done-1").await;
    // `pending-1` is left untouched at pending.

    // Resume: only the two blocked tasks move to pending.
    let (s, body) = send(&app, loop_post("/workflows/wf/resume", json!(null))).await;
    assert_eq!(s, StatusCode::OK, "resume → 200: {body}");

    assert_eq!(status_of(&store, "blocked-1").await, Status::Pending);
    assert_eq!(status_of(&store, "blocked-2").await, Status::Pending);
    // Done and pending tasks are left exactly as they were.
    assert_eq!(
        status_of(&store, "done-1").await,
        Status::Done,
        "done untouched"
    );
    assert_eq!(status_of(&store, "pending-1").await, Status::Pending);
    // With the blocked tasks reset, the workflow is no longer needs-attention.
    assert_ne!(
        body["state"], "needs-attention",
        "no blocked tasks remain: {body}"
    );

    // Unknown workflow → 404.
    let (s, _) = send(&app, loop_post("/workflows/ghost/resume", json!(null))).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "unknown workflow 404s");
}

/// Stop (pause) flips the run to `stopped` and quiesces in-flight work without
/// losing it: a `running` task is reclaimed back to `ready` (not blocked, not
/// reset). While stopped, the task-level revive verbs (retry/chat) refuse with
/// `409` — you must resume the workflow first — and resume lifts the guard.
/// Setting the auto-retry *policy* is durable config (not a revive), so it is
/// still allowed while stopped. This is the regression test for "a cancelled run
/// still lets you retry".
#[tokio::test]
async fn stop_pauses_and_blocks_revive_until_resume() {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_post(
            "/workflows",
            json!({ "id": "wf", "title": "wf", "workspace": { "repo": "/tmp/repo" } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    for tid in ["running-1", "blocked-1"] {
        let (s, _) = send(
            &app,
            loop_post(
                "/workflows/wf/tasks",
                json!({ "id": tid, "title": tid, "spec": "x" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    // One task in flight (running), one blocked.
    drive_to_running(&store, "running-1").await;
    drive_to_blocked(&store, "blocked-1", "boom").await;

    // Stop the workflow → lifecycle stopped, derived state `stopped`.
    let (s, body) = send(&app, loop_post("/workflows/wf/stop", json!(null))).await;
    assert_eq!(s, StatusCode::OK, "stop → 200: {body}");
    assert_eq!(body["state"], "stopped", "a stopped run derives `stopped`");
    // The running task was reclaimed to ready (work kept), NOT blocked or reset.
    assert_eq!(
        status_of(&store, "running-1").await,
        Status::Ready,
        "running reclaimed to ready"
    );
    let reclaimed = store.get_task("running-1").await.unwrap().unwrap();
    assert_eq!(
        reclaimed.worktree.as_deref(),
        Some("/wt"),
        "worktree kept on stop"
    );
    // The blocked task is left blocked.
    assert_eq!(status_of(&store, "blocked-1").await, Status::Blocked);

    // While stopped, the revive verbs (retry/chat) refuse with 409.
    let (s, _) = send(&app, loop_post("/tasks/blocked-1/retry", json!(null))).await;
    assert_eq!(s, StatusCode::CONFLICT, "retry refused while stopped");
    let (s, _) = send(
        &app,
        loop_post("/tasks/blocked-1/retry", json!({ "strategy": "quick" })),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "guided retry refused while stopped"
    );
    // Setting the auto-retry policy is durable config, not a revive — it is
    // allowed while stopped and takes effect on resume.
    let (s, _) = loop_put(
        &app,
        "/tasks/blocked-1/auto-retry",
        json!({ "strategy": "quick" }),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "auto-retry policy can be set while stopped"
    );
    let (s, _) = send(
        &app,
        loop_post("/tasks/blocked-1/chat", json!({ "text": "fix it" })),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "chat-revive refused while stopped");
    // The blocked task did not move — the guard truly blocked the revive.
    assert_eq!(
        status_of(&store, "blocked-1").await,
        Status::Blocked,
        "no revive happened"
    );

    // Resume → lifecycle active again; resume also resets the blocked task.
    let (s, body) = send(&app, loop_post("/workflows/wf/resume", json!(null))).await;
    assert_eq!(s, StatusCode::OK, "resume → 200: {body}");
    assert_ne!(body["state"], "stopped", "resume un-pauses the run: {body}");
    assert_eq!(
        status_of(&store, "blocked-1").await,
        Status::Pending,
        "resume reset the blocked task"
    );

    // With the run active, revive verbs work again: block a task, then retry it.
    // (`blocked-1` was reset to pending by resume, so it can be re-driven.)
    drive_to_blocked(&store, "blocked-1", "boom again").await;
    let (s, _) = send(&app, loop_post("/tasks/blocked-1/retry", json!(null))).await;
    assert_eq!(s, StatusCode::OK, "retry works once the run is resumed");
    assert_eq!(status_of(&store, "blocked-1").await, Status::Pending);
}

/// Stop & reset pauses the run AND resets unfinished tasks to `pending` (throwing
/// in-flight progress away), while keeping `done` tasks. Still resumable — not a
/// terminal tombstone.
#[tokio::test]
async fn stop_reset_pauses_and_resets_unfinished() {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_post(
            "/workflows",
            json!({ "id": "wf", "title": "wf", "workspace": { "repo": "/tmp/repo" } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    for tid in ["running-1", "blocked-1", "done-1"] {
        let (s, _) = send(
            &app,
            loop_post(
                "/workflows/wf/tasks",
                json!({ "id": tid, "title": tid, "spec": "x" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    drive_to_running(&store, "running-1").await;
    drive_to_blocked(&store, "blocked-1", "boom").await;
    drive_to_done(&store, "done-1").await;

    let (s, body) = send(&app, loop_post("/workflows/wf/stop-reset", json!(null))).await;
    assert_eq!(s, StatusCode::OK, "stop-reset → 200: {body}");
    assert_eq!(
        body["state"], "stopped",
        "stop-reset leaves the run paused, not terminal"
    );
    // Unfinished tasks reset to pending; the done task is kept.
    assert_eq!(
        status_of(&store, "running-1").await,
        Status::Pending,
        "running reset"
    );
    assert_eq!(
        status_of(&store, "blocked-1").await,
        Status::Pending,
        "blocked reset"
    );
    assert_eq!(status_of(&store, "done-1").await, Status::Done, "done kept");
    // The reset cleared the in-flight task's worktree (a clean reset, not reclaim).
    assert_eq!(
        store.get_task("running-1").await.unwrap().unwrap().worktree,
        None,
        "stop-reset clears the worktree"
    );

    // Resume brings it back to active (still no tombstone).
    let (s, body) = send(&app, loop_post("/workflows/wf/resume", json!(null))).await;
    assert_eq!(s, StatusCode::OK);
    assert_ne!(body["state"], "stopped", "resume un-pauses: {body}");
}

/// Drive `id` into `running` (claimed, with a worktree) in the store, bypassing
/// the engine — a test-only shortcut for an in-flight task.
async fn drive_to_running(store: &StoreHandle, id: &str) {
    use lazybones_store::Transition;
    store
        .transition(id, Transition::Ready, "test")
        .await
        .unwrap();
    store
        .transition(
            id,
            Transition::Claim {
                session: "s".into(),
                worktree: "/wt".into(),
                branch: "b".into(),
                base_commit: None,
            },
            "test",
        )
        .await
        .unwrap();
}

/// Drive `id` straight to `done` in the store, bypassing the engine: a test-only
/// shortcut to manufacture a terminal task (pending→ready→running→gating→done).
async fn drive_to_done(store: &StoreHandle, id: &str) {
    use lazybones_store::Transition;
    for t in [
        Transition::Ready,
        Transition::Claim {
            session: "s".into(),
            worktree: "/wt".into(),
            branch: "b".into(),
            base_commit: None,
        },
        Transition::Gate,
        Transition::Done {
            commit: "abc123".into(),
        },
    ] {
        store.transition(id, t, "test").await.unwrap();
    }
}

/// Drive `id` into `blocked` carrying a claimed worktree + `reason` — the shape a
/// gate-red failure leaves (pending→ready→running→blocked), so a guided retry has
/// a real tree to keep.
async fn drive_to_blocked(store: &StoreHandle, id: &str, reason: &str) {
    use lazybones_store::Transition;
    store
        .transition(id, Transition::Ready, "test")
        .await
        .unwrap();
    store
        .transition(
            id,
            Transition::Claim {
                session: "s".into(),
                worktree: "/wt".into(),
                branch: "b".into(),
                base_commit: None,
            },
            "test",
        )
        .await
        .unwrap();
    store
        .transition(
            id,
            Transition::Block {
                reason: reason.into(),
            },
            "test",
        )
        .await
        .unwrap();
}

/// A loop-authed `PUT` with a JSON body, returning `(status, body)`.
async fn loop_put(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("PUT")
        .uri(path)
        .header("authorization", format!("Bearer {LOOP_TOKEN}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

/// A restart with no body is a TRUE HARD RESET: it resets every task (done ones
/// included) back to `pending`, tears down their worktrees, and deletes their
/// branch locally AND on the remote — so the next run starts from a clean base.
#[tokio::test]
async fn restart_hard_reset_clears_tasks_worktrees_and_branches() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = init_repo_with_remote(tmp.path(), "repo");

    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_post(
            "/workflows",
            json!({ "id": "wf", "title": "wf", "workspace": { "repo": repo.to_str().unwrap() } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Two done tasks, each with a REAL worktree + branch pushed to origin — the
    // shape a finished isolated run leaves behind.
    let mut branches = vec![];
    for tid in ["t1", "t2"] {
        let (s, _) = send(
            &app,
            loop_post(
                "/workflows/wf/tasks",
                json!({ "id": tid, "title": tid, "spec": "x" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);

        let branch = format!("lazy/{tid}");
        let wt = repo.join(".lazy/wt").join(tid);
        git(
            &repo,
            &[
                "worktree",
                "add",
                wt.to_str().unwrap(),
                "-b",
                &branch,
                "main",
            ],
        );
        std::fs::write(wt.join(format!("{tid}.txt")), "work").unwrap();
        git(&wt, &["add", "."]);
        git(&wt, &["commit", "-m", "work"]);
        git(&wt, &["push", "origin", &branch]);
        drive_to_done_with(&store, tid, wt.to_str().unwrap(), &branch).await;
        branches.push((branch, wt));
    }

    // The run was started (so the auto-start-prevention is meaningfully tested:
    // started_at is set, and the restart must clear it).
    store.mark_run_started("wf", "2026-02-02T00:00:00Z").await.unwrap();
    assert!(store.get_run("wf").await.unwrap().unwrap().started_at.is_some());

    // Sanity: remote has both branches before the reset.
    for (branch, _) in &branches {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["ls-remote", "origin", &format!("refs/heads/{branch}")])
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&out.stdout).trim().is_empty(),
            "{branch} on remote pre-reset"
        );
    }

    // Hard reset (empty body → soft:false).
    let (s, body) = send(&app, loop_post("/workflows/wf/restart", json!(null))).await;
    assert_eq!(s, StatusCode::OK, "restart → 200: {body}");
    // Must NOT auto-start: the run un-activates (started_at cleared) so it derives
    // back to `draft` and waits for an explicit Start — not re-run on the next tick.
    assert_eq!(body["state"], "draft", "restart does not auto-start: {body}");
    assert!(
        store.get_run("wf").await.unwrap().unwrap().started_at.is_none(),
        "restart clears started_at so the scheduler skips the run until Start"
    );

    // Every task — done included — is back to pending with no worktree.
    for tid in ["t1", "t2"] {
        assert_eq!(
            status_of(&store, tid).await,
            Status::Pending,
            "{tid} reset to pending"
        );
        assert_eq!(
            store.get_task(tid).await.unwrap().unwrap().worktree,
            None,
            "{tid} worktree cleared"
        );
    }

    // Branches gone locally AND on the remote → a re-run starts clean.
    for (branch, wt) in &branches {
        assert!(!wt.exists(), "worktree dir {} removed", wt.display());
        let local = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ])
            .output()
            .unwrap();
        assert!(!local.status.success(), "local branch {branch} deleted");
        let remote = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["ls-remote", "origin", &format!("refs/heads/{branch}")])
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&remote.stdout).trim().is_empty(),
            "remote branch {branch} deleted on hard reset"
        );
    }
}

/// A `soft` restart keeps done tasks and their worktrees/branches — the
/// resume-style escape hatch (only the unfinished part is redone).
#[tokio::test]
async fn restart_soft_keeps_done_tasks_and_worktrees() {
    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();
    let app = router(AppState::new(
        store.clone(),
        "run",
        "http://127.0.0.1:0",
        LOOP_TOKEN,
    ));

    let (s, _) = send(
        &app,
        loop_post(
            "/workflows",
            json!({ "id": "wf", "title": "wf", "workspace": { "repo": "/tmp/repo" } }),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    for tid in ["blocked-1", "done-1"] {
        let (s, _) = send(
            &app,
            loop_post(
                "/workflows/wf/tasks",
                json!({ "id": tid, "title": tid, "spec": "x" }),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    drive_to_blocked(&store, "blocked-1", "boom").await;
    drive_to_done(&store, "done-1").await;

    let (s, body) = send(
        &app,
        loop_post("/workflows/wf/restart", json!({ "soft": true })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "soft restart → 200: {body}");
    // Unfinished part reset; done task kept (resume semantics).
    assert_eq!(
        status_of(&store, "blocked-1").await,
        Status::Pending,
        "blocked reset"
    );
    assert_eq!(
        status_of(&store, "done-1").await,
        Status::Done,
        "done kept on soft restart"
    );
    // The done task's worktree is left in place for reuse.
    assert_eq!(
        store
            .get_task("done-1")
            .await
            .unwrap()
            .unwrap()
            .worktree
            .as_deref(),
        Some("/wt"),
        "soft restart keeps the worktree"
    );
}

/// Drive `id` to `done` carrying a REAL worktree path + branch (vs `drive_to_done`'s
/// placeholder `/wt`/`b`), so the restart's git teardown has something real to act on.
async fn drive_to_done_with(store: &StoreHandle, id: &str, worktree: &str, branch: &str) {
    use lazybones_store::Transition;
    for t in [
        Transition::Ready,
        Transition::Claim {
            session: "s".into(),
            worktree: worktree.into(),
            branch: branch.into(),
            base_commit: None,
        },
        Transition::Gate,
        Transition::Done {
            commit: "abc123".into(),
        },
    ] {
        store.transition(id, t, "test").await.unwrap();
    }
}
