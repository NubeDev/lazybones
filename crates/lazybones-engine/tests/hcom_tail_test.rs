//! The TAIL phase end-to-end: a fake `hcom` reports a live agent (via `list`) and
//! a queued event (via `events --wait 0`); one tick ingests it into the durable
//! hcom log keyed to its `(run, task)` and advances the run's cursor.
//!
//! The fake hcom answers `list --json` with one tagged agent and `events` with a
//! single message for that agent. Launch isn't exercised (no ready tasks), so the
//! tick is reclaim → promote → claim(noop) → tail.

use std::path::Path;

use lazybones_engine::{EngineConfig, MergeMode, harness::Engine};
use lazybones_store::{
    HcomLogFilter, Run, StoreEngine, StoreHandle, Task, Workspace, WorktreeMode,
};

/// A fake hcom mirroring the real 0.7.21 shapes that bit us:
/// - `list --json` reports the agent with a *prefixed* `name` (`auth-kula`) and a
///   short `base_name` (`kula`), tagged with task `auth`;
/// - `events` returns one message whose `instance` is the *base name* (`kula`),
///   not the full name — the real stream keys events by base name.
///
/// The tail must bridge the two (key its name→tag map on `base_name`) or the event
/// is dropped, which is exactly what left the UI empty. Anything else exits 0.
fn write_fake_hcom(dir: &Path) -> String {
    let path = dir.join("hcom");
    let script = r#"#!/bin/sh
case "$1" in
  list)
    echo '[{"name":"auth-kula","base_name":"kula","status":"active","tag":"auth"}]'
    ;;
  events)
    echo '{"id":7,"ts":"2026-01-01T00:00:00Z","type":"message","instance":"kula","data":{"text":"working on auth"}}'
    ;;
  *)
    echo "Names: kula"
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
        auto_trust_agent_folder: true,
        stale_after_secs: 300,
        tick_secs: 1,
        issue_sync_every_n_ticks: 0,
    }
}

#[tokio::test]
async fn tail_ingests_event_and_advances_cursor() {
    let tmp = tempfile::tempdir().unwrap();
    let hcom_bin = write_fake_hcom(tmp.path());

    let store = StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "key")
        .await
        .unwrap();

    // An active workflow and a task linked to it (run_id = workflow-1). The task
    // is `running` with a heartbeat so reclaim leaves it alone (a live agent
    // carries its tag anyway).
    let run = Run::new(
        "workflow-1",
        "WF",
        Workspace {
            repo: "/repo".into(),
            base_branch: None,
            branch_prefix: None,
            worktree_mode: WorktreeMode::New,
            tool: None,
            model: None,
            effort: None,
            gate: None,
            merge: None,
            auto_pr: None,
        },
        "2026-01-01T00:00:00Z",
    );
    store.create_run(&run).await.unwrap();

    let mut task = Task::seed("auth", "wf-run", "Auth", "build auth", vec![], vec![], None);
    task.run_id = Some("workflow-1".into());
    store.create_task(&task).await.unwrap();

    let engine = Engine::with_hcom_bin(store.clone(), engine_cfg(tmp.path()), &hcom_bin);
    engine.tick().await;

    // The event landed in the hcom log keyed to the *workflow id* (run_id =
    // workflow-1), NOT the task's `run` label (`wf-run`): the workflow detail page
    // and `GET /runs/:id/hcom` are keyed on the workflow id, so the tail stores the
    // row there. (Keying on the label was the bug that left the UI empty.)
    let log = store
        .run_hcom_log("workflow-1", &HcomLogFilter::default())
        .await
        .unwrap();
    assert_eq!(log.len(), 1, "one event ingested");
    let entry = &log[0];
    assert_eq!(entry.hcom_id, 7);
    assert_eq!(entry.task.as_deref(), Some("auth"));
    assert_eq!(entry.agent, "kula");
    assert_eq!(entry.tag.as_deref(), Some("auth"));
    assert_eq!(entry.kind, "message");
    assert_eq!(entry.data["text"], serde_json::json!("working on auth"));

    // The run's cursor advanced to the max ingested id.
    let cursor = store
        .get_run("workflow-1")
        .await
        .unwrap()
        .unwrap()
        .hcom_log_cursor;
    assert_eq!(cursor, Some(7));

    // A second tick re-drains from below the cursor but the (run, hcom_id) upsert
    // keeps it idempotent — still exactly one row.
    engine.tick().await;
    let log = store
        .run_hcom_log("workflow-1", &HcomLogFilter::default())
        .await
        .unwrap();
    assert_eq!(log.len(), 1, "re-ingest is idempotent");
}
