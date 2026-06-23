//! The task lifecycle, end to end, over an in-memory store.

use lazybones_store::{SeedTask, StoreEngine, StoreError, StoreHandle, Transition, sync_seeds};

async fn store() -> StoreHandle {
    StoreHandle::open(&StoreEngine::Memory, "lazybones", "test", "test-secret-key")
        .await
        .expect("open in-memory store")
}

fn seed(id: &str, deps: Vec<String>) -> SeedTask {
    SeedTask {
        id: id.to_owned(),
        title: format!("task {id}"),
        spec: format!("spec for {id}"),
        deps,
        owns: vec![],
        tool: None,
        reuse_from: None,
    }
}

#[tokio::test]
async fn claim_gate_done_walks_the_happy_path() {
    let store = store().await;
    sync_seeds(&store, "run", &[seed("store", vec![])])
        .await
        .unwrap();

    // No-dep task is immediately ready.
    let ready = store.newly_ready(&[]).await.unwrap();
    assert_eq!(ready, vec!["store".to_owned()]);
    store
        .transition("store", Transition::Ready, "loop")
        .await
        .unwrap();

    store
        .transition(
            "store",
            Transition::Claim {
                session: "sess-1".into(),
                worktree: "/wt/store".into(),
                branch: "lazy/store".into(),
                base_commit: None,
            },
            "loop",
        )
        .await
        .unwrap();
    store
        .transition("store", Transition::Gate, "loop")
        .await
        .unwrap();
    let done = store
        .transition(
            "store",
            Transition::Done {
                commit: "abc123".into(),
            },
            "loop",
        )
        .await
        .unwrap();

    assert_eq!(done.status, lazybones_store::Status::Done);
    assert_eq!(done.commit.as_deref(), Some("abc123"));

    // The run log captured every transition.
    let history = store.run_history("run").await.unwrap();
    let path: Vec<_> = history
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    assert_eq!(
        path,
        vec![
            ("pending", "ready"),
            ("ready", "running"),
            ("running", "gating"),
            ("gating", "done"),
        ]
    );
}

#[tokio::test]
async fn illegal_transition_is_rejected() {
    let store = store().await;
    sync_seeds(&store, "run", &[seed("store", vec![])])
        .await
        .unwrap();

    // pending -> done is not a legal move.
    let err = store
        .transition("store", Transition::Done { commit: "x".into() }, "loop")
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::IllegalTransition { .. }));
}

#[tokio::test]
async fn dependent_task_is_not_ready_until_dep_done() {
    let store = store().await;
    sync_seeds(
        &store,
        "run",
        &[seed("store", vec![]), seed("api", vec!["store".into()])],
    )
    .await
    .unwrap();

    // Only `store` is ready at first.
    assert_eq!(store.newly_ready(&[]).await.unwrap(), vec!["store".to_owned()]);

    // Drive `store` to done.
    for t in [
        Transition::Ready,
        Transition::Claim {
            session: "s".into(),
            worktree: "w".into(),
            branch: "b".into(),
            base_commit: None,
        },
        Transition::Gate,
        Transition::Done { commit: "c".into() },
    ] {
        store.transition("store", t, "loop").await.unwrap();
    }

    // Now `api` becomes ready.
    assert_eq!(store.newly_ready(&[]).await.unwrap(), vec!["api".to_owned()]);
}

#[tokio::test]
async fn reuse_from_implies_a_dependency_edge() {
    let store = store().await;
    // `consumer` reuses `producer`'s worktree but lists NO explicit deps. The
    // reuse link alone must order them: consumer stays pending until producer is
    // done, exactly as if it had `depends_on: [producer]`.
    let mut consumer = seed("consumer", vec![]);
    consumer.reuse_from = Some("producer".into());
    sync_seeds(&store, "run", &[seed("producer", vec![]), consumer])
        .await
        .unwrap();

    // Only `producer` is ready — the implied edge holds `consumer` back.
    assert_eq!(
        store.newly_ready(&[]).await.unwrap(),
        vec!["producer".to_owned()]
    );

    for t in [
        Transition::Ready,
        Transition::Claim {
            session: "s".into(),
            worktree: "w".into(),
            branch: "b".into(),
            base_commit: None,
        },
        Transition::Gate,
        Transition::Done { commit: "c".into() },
    ] {
        store.transition("producer", t, "loop").await.unwrap();
    }

    assert_eq!(
        store.newly_ready(&[]).await.unwrap(),
        vec!["consumer".to_owned()]
    );
}

#[tokio::test]
async fn transitions_are_published_on_the_live_bus() {
    let store = store().await;
    sync_seeds(&store, "run", &[seed("store", vec![])])
        .await
        .unwrap();

    // Subscribe BEFORE the transition — the bus replays nothing, only live events.
    let mut feed = store.subscribe();

    store
        .transition("store", Transition::Ready, "loop")
        .await
        .unwrap();

    let live = feed.recv().await.expect("a published transition");
    let lazybones_store::LiveEvent::Transition(event) = live else {
        panic!("expected a transition, got {live:?}");
    };
    assert_eq!(event.task, "store");
    assert_eq!(event.from, "pending");
    assert_eq!(event.to, "ready");
    assert_eq!(event.actor, "loop");
}

#[tokio::test]
async fn resync_preserves_lifecycle() {
    let store = store().await;
    sync_seeds(&store, "run", &[seed("store", vec![])])
        .await
        .unwrap();
    store
        .transition("store", Transition::Ready, "loop")
        .await
        .unwrap();

    // Re-import the same task: it must NOT reset to pending.
    sync_seeds(&store, "run", &[seed("store", vec![])])
        .await
        .unwrap();
    let task = store.get_task("store").await.unwrap().unwrap();
    assert_eq!(task.status, lazybones_store::Status::Ready);
}
