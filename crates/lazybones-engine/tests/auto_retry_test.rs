//! Scheduler auto-retry: a task with an `auto_retry` policy whose agent keeps
//! signalling BLOCKED is re-attempted by the scheduler up to `max_retries` times
//! (each time with the strategy's guidance folded into its conversation), then
//! left blocked for a human once the budget is spent.
//!
//! Same harness shape as `tick_walk_test`, but the fake hcom emits BLOCKED rather
//! than DONE so every claim ends in a block — exercising the auto-retry path in
//! `scheduler::finish::block`.

use std::path::Path;
use std::process::Command;

use lazybones_engine::{EngineConfig, MergeMode, harness::Engine};
use lazybones_store::{RetryStrategy, Status, StoreEngine, StoreHandle, Task};

/// A `hcom` stub whose agent always reports BLOCKED (so each claim ends blocked).
fn write_blocking_hcom(dir: &Path) -> String {
    let path = dir.join("hcom");
    let script = r#"#!/bin/sh
case "$1" in
  list)
    echo '[]'
    ;;
  events)
    echo '{"id":1,"ts":"2026-01-01T00:00:00Z","type":"message","instance":"testagent","data":{"text":"BLOCKED: still red","thread":"x"}}'
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
        gate: vec!["true".into()],
        concurrency: 3,
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

#[tokio::test]
async fn auto_retry_reattempts_up_to_the_cap_then_stays_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = init_repo_with_remote(tmp.path());
    let hcom_bin = write_blocking_hcom(tmp.path());

    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();

    // One task with auto-retry on (quick), a tight cap of 1: it gets exactly one
    // hands-off re-attempt after the first block, then stays blocked.
    let mut t = Task::seed("flaky", "r", "Flaky", "build it", vec![], vec![], None);
    t.auto_retry = Some(RetryStrategy::Quick);
    t.max_retries = 1;
    store.create_task(&t).await.unwrap();

    let engine = Engine::with_hcom_bin(store.clone(), engine_cfg(&repo), &hcom_bin);

    // Tick 1: promote → claim → spawn. The agent reports BLOCKED; the scheduler
    // blocks the task, sees the auto-retry budget (0 < 1), and revives it (→ ready,
    // retry_count → 1, guidance appended). So after the dust settles it is back to
    // ready (or already re-running), NOT terminally blocked.
    engine.tick().await;
    assert!(
        wait_for(&store, "flaky", Status::Ready).await
            || status_of(&store, "flaky").await == Status::Running,
        "first block should auto-revive, not stay blocked; got {:?}",
        status_of(&store, "flaky").await
    );

    // The first auto-retry recorded its guidance in the conversation and bumped
    // the counter to 1.
    let after_first = store.get_task("flaky").await.unwrap().unwrap();
    assert_eq!(after_first.retry_count, 1, "one auto-retry spent");
    let chat = store.chat_history("flaky").await.unwrap();
    assert_eq!(chat.len(), 1, "one guidance message after the first auto-retry");
    assert!(
        chat[0].text.contains("still red") && chat[0].text.contains("smallest"),
        "quick-strategy guidance names the reason: {:?}",
        chat[0].text
    );

    // Tick 2: re-claim the revived task → agent BLOCKs again → budget now spent
    // (retry_count 1 >= cap 1) → it stays blocked for a human.
    engine.tick().await;
    assert!(
        wait_for(&store, "flaky", Status::Blocked).await,
        "second block exhausts the budget and stays blocked; got {:?}",
        status_of(&store, "flaky").await
    );

    // The counter did not exceed the cap, and no further guidance was appended.
    let final_task = store.get_task("flaky").await.unwrap().unwrap();
    assert_eq!(final_task.retry_count, 1, "no auto-retry past the cap");
    assert_eq!(
        store.chat_history("flaky").await.unwrap().len(),
        1,
        "no second guidance message once the budget is spent"
    );
}
