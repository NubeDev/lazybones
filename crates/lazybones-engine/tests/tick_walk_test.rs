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
use lazybones_store::{Status, StoreEngine, StoreHandle, Task};

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
