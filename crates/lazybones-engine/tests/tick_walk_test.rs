//! End-to-end tick walk: a `store → auth` dependency, a fake `hcom` on `PATH`,
//! and a tempdir git repo. Asserts the dependency cascade and the lifecycle
//! `pending → ready → running → gating → done`, with `auth` starting only after
//! `store` is `done`.
//!
//! The scheduler is driven one tick at a time via the `harness::Engine` test
//! seam (the production loop ticks forever). The fake hcom binary answers the
//! three subcommands the scheduler uses: launch (`Names:`), `events --wait`
//! (a DONE signal), and `list --json` (no live agents).

use std::path::Path;
use std::process::Command;

use lazybones_engine::{EngineConfig, MergeMode, harness::Engine};
use lazybones_store::{
    Run, Status, StoreEngine, StoreHandle, Task, WorktreeMode, Workspace,
};

/// Write a `hcom` stub that prints `Names: testagent` on launch, emits a DONE
/// event on `events --wait`, and `[]` on `list --json`. Returns its path.
fn write_fake_hcom(dir: &Path) -> String {
    let path = dir.join("hcom");
    let script = r#"#!/bin/sh
# Fake hcom for the scheduler integration test.
case "$1" in
  list)
    echo '[]'
    ;;
  events)
    # Block-until-match: immediately report a DONE message event.
    echo '{"id":1,"ts":"2026-01-01T00:00:00Z","type":"message","instance":"testagent","data":{"text":"DONE","thread":"x"}}'
    ;;
  *)
    # A launch: `1 <tool> --tag <id> ...`. Print the spawned name.
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

/// `git` in `dir` with `args`, asserting success.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A git repo with one commit and a local bare remote `origin` so the merge
/// push step succeeds. Returns the work repo path.
fn init_repo_with_remote(root: &Path) -> std::path::PathBuf {
    let bare = root.join("remote.git");
    Command::new("git").args(["init", "--bare"]).arg(&bare).output().unwrap();

    let repo = root.join("work");
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

fn engine_cfg(repo: &Path) -> EngineConfig {
    EngineConfig {
        target_repo: repo.to_path_buf(),
        base_branch: "main".into(),
        remote: "origin".into(),
        // A trivial green gate — the agent work is faked, so we only verify the
        // state machine, not a real build.
        gate: vec!["true".into()],
        concurrency: 3,
        // Serial / branch mode: one checkout, no real worktrees, so the faked
        // agent's empty branch fast-forwards cleanly into main.
        worktrees: false,
        worktree_root: ".lazy/wt".into(),
        branch_prefix: "lazy/".into(),
        merge: MergeMode::FastForward,
        agent_tool: "claude".into(),
        agent_model: None,
        agent_effort: None,
        permission_flags: std::collections::HashMap::new(),
        stale_after_secs: 300,
        tick_secs: 1,
    }
}

async fn status_of(store: &StoreHandle, id: &str) -> Status {
    store.get_task(id).await.unwrap().unwrap().status
}

#[tokio::test]
async fn task_walks_lifecycle_and_respects_dependency() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = init_repo_with_remote(tmp.path());
    let hcom_bin = write_fake_hcom(tmp.path());

    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();

    // Seed `store` (no deps) and `auth` (depends on `store`), both pending.
    store
        .create_task(&Task::seed("store", "r", "Store", "build store", vec![], vec![], None))
        .await
        .unwrap();
    let mut auth = Task::seed("auth", "r", "Auth", "build auth", vec!["store".into()], vec![], None);
    auth.deps = vec!["store".into()];
    store.create_task(&auth).await.unwrap();
    store.relate_dep("auth", "store").await.unwrap();

    let engine = Engine::with_hcom_bin(store.clone(), engine_cfg(&repo), &hcom_bin);

    // Tick 1: `store` promotes pending→ready→running (claimed + spawned); `auth`
    // stays pending because `store` is not yet done.
    engine.tick().await;
    assert_eq!(status_of(&store, "store").await, Status::Running);
    assert_eq!(status_of(&store, "auth").await, Status::Pending, "auth must wait for store");

    // The spawned finish task awaits DONE, gates, merges, and records done. Poll
    // until `store` reaches a terminal state.
    let store_done = wait_for(&store, "store", Status::Done).await;
    assert!(store_done, "store should reach done; got {:?}", status_of(&store, "store").await);

    // Tick 2: now that `store` is done, `auth` promotes and is claimed.
    engine.tick().await;
    assert_eq!(status_of(&store, "auth").await, Status::Running, "auth starts after store done");
    let auth_done = wait_for(&store, "auth", Status::Done).await;
    assert!(auth_done, "auth should reach done; got {:?}", status_of(&store, "auth").await);
}

/// A workspace pointing at `repo` with all-inherited git config.
fn workspace(repo: &Path) -> Workspace {
    Workspace {
        repo: repo.to_string_lossy().into_owned(),
        base_branch: None,
        branch_prefix: None,
        worktree_mode: WorktreeMode::New,
        tool: None,
        model: None,
        effort: None,
        gate: None,
        merge: None,
    }
}

/// A stopped (paused) workflow must promote/claim nothing: a `ready` task whose
/// parent run is `stopped` stays `ready` across a tick — no agent is spawned.
/// This is the regression guard for the "cancelled run still runs" bug.
#[tokio::test]
async fn stopped_run_claims_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = init_repo_with_remote(tmp.path());
    let hcom_bin = write_fake_hcom(tmp.path());

    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();

    // A workflow with one task already `ready`, then paused.
    let run = Run::new("wf-stopped", "Stopped WF", workspace(&repo), "2026-01-01T00:00:00Z");
    store.create_run(&run).await.unwrap();
    let mut t = Task::seed("solo", "wf-stopped", "Solo", "build it", vec![], vec![], None);
    t.run_id = Some("wf-stopped".into());
    t.status = Status::Ready;
    store.create_task(&t).await.unwrap();
    // The run was started (operator pressed Start) before being paused — only a
    // started run can be stopped/resumed. Stamp `started_at` so the post-resume
    // claim isn't blocked by the "created-but-never-started promotes nothing" guard.
    store.mark_run_started("wf-stopped", "2026-01-01T00:00:01Z").await.unwrap();
    store.stop_run("wf-stopped").await.unwrap();

    let engine = Engine::with_hcom_bin(store.clone(), engine_cfg(&repo), &hcom_bin);

    // A full tick: reconcile → promote → claim. The stopped run must be skipped
    // at claim, so the task is never moved off `ready`.
    engine.tick().await;
    assert_eq!(
        status_of(&store, "solo").await,
        Status::Ready,
        "a stopped run's ready task must not be claimed"
    );

    // Give any (erroneously) spawned finish task a moment; it must still not run.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert_eq!(
        status_of(&store, "solo").await,
        Status::Ready,
        "still not claimed after a grace window"
    );

    // Resume flips the run active; now the same task is claimed on the next tick.
    // (The fake agent is instant, so it may already have raced past `running` to
    // `gating`/`done` — the point is it left `ready`, i.e. it was claimed.)
    store.resume_run("wf-stopped").await.unwrap();
    engine.tick().await;
    let after = status_of(&store, "solo").await;
    assert!(
        matches!(after, Status::Running | Status::Gating | Status::Done),
        "after resume the task is claimed and an agent spawned; got {after:?}"
    );
}

/// A created-but-never-started workflow must promote/claim nothing: its root
/// task whose deps are all satisfied stays `pending` across a tick, because no
/// operator has pressed Start (the run's `started_at` is still `null`).
/// Regression guard for "creating a workflow auto-ran it."
#[tokio::test]
async fn unstarted_run_promotes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = init_repo_with_remote(tmp.path());
    let hcom_bin = write_fake_hcom(tmp.path());

    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();

    // An active workflow with a ready-to-promote root task, but never started.
    let run = Run::new("wf-unstarted", "Unstarted WF", workspace(&repo), "2026-01-01T00:00:00Z");
    store.create_run(&run).await.unwrap();
    assert!(run.started_at.is_none(), "a fresh run has no started_at");
    let mut t = Task::seed("root", "wf-unstarted", "Root", "build it", vec![], vec![], None);
    t.run_id = Some("wf-unstarted".into());
    store.create_task(&t).await.unwrap();

    let engine = Engine::with_hcom_bin(store.clone(), engine_cfg(&repo), &hcom_bin);

    // A full tick must leave the task untouched: not promoted, not claimed.
    engine.tick().await;
    assert_eq!(
        status_of(&store, "root").await,
        Status::Pending,
        "an unstarted run's root task must not be promoted"
    );

    // Start it (the operator's "go") and the same task promotes on the next tick.
    store.mark_run_started("wf-unstarted", "2026-01-01T00:00:01Z").await.unwrap();
    engine.tick().await;
    assert_ne!(
        status_of(&store, "root").await,
        Status::Pending,
        "after start the task leaves pending"
    );
}

/// Poll a task's status up to ~5s, returning whether it reached `target`.
async fn wait_for(store: &StoreHandle, id: &str, target: Status) -> bool {
    for _ in 0..100 {
        if status_of(store, id).await == target {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}
