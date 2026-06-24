//! The supervisor loop: tick forever until the task is aborted.
//!
//! `serve()` spawns this next to axum and `abort()`s it on shutdown. It owns no
//! in-memory run state — every tick rebuilds reality from the store + `hcom list`
//! + git, so it is safe to kill and resume at any point (SCOPE.md restart).

use std::time::Duration;

use lazybones_store::StoreHandle;

use crate::config::EngineConfig;
use crate::hcom::Hcom;

use super::ext::{ExtHooks, spawn_event_reactions};
use super::tick::tick;

/// Run the scheduler against `store` until aborted, with no extensions wired.
///
/// The agent CLI credentials are loaded once from the store and exported into
/// every spawned agent. (A credential added mid-run is picked up on the next
/// daemon restart — acceptable for v1.)
pub async fn run(store: StoreHandle, cfg: EngineConfig) {
    run_with_ext(store, cfg, ExtHooks::none()).await;
}

/// Run the scheduler with an [`ExtHooks`] wired in (the daemon path): gate-check
/// extensions run at the gate point (fail-closed) and the event-reaction loop runs
/// off the durable event stream (fail-open, cycle-guarded — design §3.2/§3.4).
pub async fn run_with_ext(store: StoreHandle, cfg: EngineConfig, ext: ExtHooks) {
    let hcom = build_hcom(&store).await;
    let period = Duration::from_secs(cfg.tick_secs.max(1));
    tracing::info!(
        tick_secs = cfg.tick_secs,
        concurrency = cfg.concurrency,
        extensions = ext.is_enabled(),
        "scheduler started"
    );

    // The fail-open event-reaction loop lives in its own task, off the tick path,
    // so a reaction fault/loop/kill can never wedge a scheduler tick.
    spawn_event_reactions(store.clone(), &ext);

    // The set of tasks this process is driving — shared into every tick so the
    // recovery pass can re-attach drive loops to in-flight tasks (e.g. after this
    // daemon restarted) without ever double-driving one already in flight.
    let driving = super::finish::Driving::default();

    let mut ticker = tokio::time::interval(period);
    let mut tick_count: u64 = 0;
    loop {
        ticker.tick().await;
        tick(&store, &hcom, &cfg, tick_count, &driving, &ext).await;
        tick_count = tick_count.wrapping_add(1);
    }
}

/// Build the hcom client, exporting the stored agent credentials.
async fn build_hcom(store: &StoreHandle) -> Hcom {
    let env = match store.secret_env().await {
        Ok(pairs) => pairs
            .into_iter()
            .map(|s| (s.env_var, s.value))
            .collect::<Vec<_>>(),
        Err(e) => {
            tracing::warn!("scheduler: loading secret env failed, spawning without it: {e}");
            Vec::new()
        }
    };
    Hcom::discover().with_env(env)
}
