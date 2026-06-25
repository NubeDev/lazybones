//! The auto-sync drivers: a boot-time auto-pull and a periodic auto-push, both
//! gated on the operator's [`SyncConfig`] master switch + per-behaviour flag.
//!
//! These run the *same* [`PullJob`]/[`PushJob`] the API drives, through the
//! shared [`JobRunner`] — they just decide *when*. Keeping the "when" here (off a
//! re-read of preferences each tick) means toggling auto-sync in the UI takes
//! effect on the next tick without a restart.

use std::path::PathBuf;
use std::time::Duration;

use lazybones_jobs::JobRunner;
use lazybones_store::StoreHandle;

use super::{PULL_JOB, PUSH_JOB};

/// How often the auto-push loop wakes to check for changes to push. Coarse on
/// purpose: a push is a network round-trip and the manual button + the
/// out-of-sync banner cover the "I need it now" case.
pub const AUTO_PUSH_INTERVAL: Duration = Duration::from_secs(120);

/// Run the boot auto-pull once, if the operator enabled it. Best-effort: a
/// slow/unreachable remote logs a warning rather than failing the daemon. Call
/// this spawned so it never blocks serving.
pub async fn auto_pull_on_boot(store: StoreHandle, runner: JobRunner) {
    if !auto_pull_wanted(&store).await {
        return;
    }
    match runner.run_now(PULL_JOB).await {
        Ok(report) => tracing::info!(summary = %report.summary, "boot auto-pull complete"),
        Err(e) => tracing::warn!("boot auto-pull failed: {e}"),
    }
}

/// The periodic auto-push loop: every [`AUTO_PUSH_INTERVAL`], if the operator has
/// auto-push enabled, run the push job (a no-op commit when nothing changed, so
/// an idle store pushes nothing). Re-reads preferences each tick so the toggle is
/// live. Runs until the task is aborted (the daemon owns its handle).
pub async fn auto_push_loop(store: StoreHandle, runner: JobRunner) {
    let mut ticker = tokio::time::interval(AUTO_PUSH_INTERVAL);
    // Skip the immediate first tick: nothing has changed at boot, and a boot
    // auto-pull may still be in flight.
    ticker.tick().await;
    loop {
        ticker.tick().await;
        if !auto_push_wanted(&store).await {
            continue;
        }
        match runner.run_now(PUSH_JOB).await {
            Ok(report) => tracing::debug!(summary = %report.summary, "auto-push tick"),
            Err(e) => tracing::warn!("auto-push tick failed: {e}"),
        }
    }
}

/// Spawn the auto-push loop as a background task, returning its handle so the
/// daemon can abort it on shutdown. The local sync dir is derived per-tick from
/// preferences, so `_data_dir` isn't needed here — the jobs already carry it.
#[must_use]
pub fn spawn_auto_push(
    store: StoreHandle,
    runner: JobRunner,
    _data_dir: impl Into<PathBuf>,
) -> tokio::task::JoinHandle<()> {
    let _data_dir = _data_dir.into();
    tokio::spawn(auto_push_loop(store, runner))
}

async fn auto_pull_wanted(store: &StoreHandle) -> bool {
    sync_cfg(store)
        .await
        .is_some_and(|c| c.auto_pull_active())
}

async fn auto_push_wanted(store: &StoreHandle) -> bool {
    sync_cfg(store)
        .await
        .is_some_and(|c| c.auto_push_active())
}

/// Read just the sync config out of preferences (None on any read error so the
/// loop quietly does nothing rather than spamming logs).
async fn sync_cfg(store: &StoreHandle) -> Option<lazybones_store::SyncConfig> {
    store.get_preferences().await.ok().flatten().and_then(|p| p.sync)
}
