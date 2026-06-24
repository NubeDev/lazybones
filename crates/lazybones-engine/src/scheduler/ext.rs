//! Scheduler-side extension dispatch wiring (extension-system design §3.2, §3.4).
//!
//! `lazybones-ext` owns all WASM concerns; this module is the thin seam that lets
//! the scheduler *use* them without pulling Wasmtime into its own code:
//!
//! - **Gate-check** runs at the existing gate point, **FAIL-CLOSED** — an
//!   extension that blocks (or faults) blocks the land, layered on top of the
//!   command gate. See [`ExtHooks::gate_check`].
//! - **Event-reaction** runs off the durable `Transition`/SSE event stream,
//!   **FAIL-OPEN** and cycle-guarded — see [`spawn_event_reactions`]. It lives in
//!   its **own** tokio task subscribed to the store's event bus, completely off the
//!   tick path, so a reaction that faults, loops, or is killed can never wedge a
//!   scheduler tick (the "every tick rebuilds from store + git + hcom" invariant
//!   is preserved).
//!
//! The whole thing is gated behind an [`ExtHooks`] that is `None` when no
//! dispatcher is wired: the extension-free daemon and the test harness then pay
//! nothing and behave exactly as before.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use lazybones_ext::{ComponentLoader, DiffStat, Dispatcher, GateDecision, GateInput, HostEvent};
use lazybones_store::{BlobStore, LiveEvent, StoreHandle, Task};

/// The blob-store project the installed `.wasm` component bytes live under
/// (mirrors the `/extensions` install route).
const EXT_BLOB_PROJECT: &str = "extensions";

/// A cloneable handle the scheduler threads through every tick. `None` means
/// extensions are disabled (no dispatcher wired); every method is then a cheap
/// no-op, so the common no-extensions path is free.
#[derive(Clone, Default)]
pub struct ExtHooks {
    dispatcher: Option<Arc<Dispatcher>>,
}

impl ExtHooks {
    /// No extensions wired — every hook is a no-op (the default for the test
    /// harness and an extension-free daemon).
    #[must_use]
    pub fn none() -> Self {
        Self { dispatcher: None }
    }

    /// Wire a shared [`Dispatcher`] into the scheduler.
    #[must_use]
    pub fn new(dispatcher: Arc<Dispatcher>) -> Self {
        Self {
            dispatcher: Some(dispatcher),
        }
    }

    /// Whether any dispatcher is wired.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.dispatcher.is_some()
    }

    /// The shared dispatcher, if wired.
    #[must_use]
    pub fn dispatcher(&self) -> Option<&Arc<Dispatcher>> {
        self.dispatcher.as_ref()
    }

    /// Run all enabled **gate-check** extensions for `task`'s candidate worktree,
    /// FAIL-CLOSED. Returns [`GateDecision::Pass`] when no dispatcher is wired (the
    /// command gate is untouched). A failing/faulting extension yields
    /// [`GateDecision::Block`], which the caller turns into a task block.
    pub async fn gate_check(&self, task: &Task, worktree: &Path) -> GateDecision {
        let Some(dispatcher) = &self.dispatcher else {
            return GateDecision::Pass;
        };
        let input = gate_input(task, worktree).await;
        dispatcher.run_gate_checks(input).await
    }
}

/// Build the gate-check input for a task: its id + a one-line summary + a rolled-up
/// diff stat for the candidate worktree (design §3.2). The diff stat is best-effort
/// — a git failure yields zeros, never an error, so a gate check still runs.
async fn gate_input(task: &Task, worktree: &Path) -> GateInput {
    let diff = diff_stat(worktree, task.base_commit.as_deref()).await;
    GateInput {
        task_id: task.id.clone(),
        task_summary: task.title.clone(),
        diff,
    }
}

/// Roll up a `git diff --numstat` for the worktree into a [`DiffStat`]. When
/// `base` is set we diff against it (captures committed + uncommitted work since
/// the task was claimed); otherwise we diff the working tree against `HEAD`.
/// Best-effort: any git/parse failure returns a zero stat.
async fn diff_stat(worktree: &Path, base: Option<&str>) -> DiffStat {
    let mut args = vec!["diff", "--numstat"];
    if let Some(base) = base {
        args.push(base);
    }
    let out = match super::git::git(worktree, &args).await {
        Ok(o) if o.ok => o,
        _ => return zero_stat(),
    };

    let mut files = 0u32;
    let mut insertions = 0u32;
    let mut deletions = 0u32;
    for line in out.stdout.lines() {
        // `<added>\t<deleted>\t<path>`; binary files show `-` for the counts.
        let mut cols = line.split('\t');
        let added = cols.next().unwrap_or("0");
        let deleted = cols.next().unwrap_or("0");
        if cols.next().is_none() {
            continue; // not a numstat row
        }
        files += 1;
        insertions = insertions.saturating_add(added.parse::<u32>().unwrap_or(0));
        deletions = deletions.saturating_add(deleted.parse::<u32>().unwrap_or(0));
    }
    DiffStat {
        files_changed: files,
        insertions,
        deletions,
    }
}

/// A zeroed diff stat (the generated WIT record has no `Default`).
fn zero_stat() -> DiffStat {
    DiffStat {
        files_changed: 0,
        insertions: 0,
        deletions: 0,
    }
}

/// A [`ComponentLoader`] backed by the daemon's content-addressed blob store — the
/// same store the `/extensions` install route writes the `.wasm` bytes into.
pub struct BlobComponentLoader {
    assets: Arc<dyn BlobStore>,
}

impl BlobComponentLoader {
    /// Load components from `assets` (the shared asset/blob store).
    #[must_use]
    pub fn new(assets: Arc<dyn BlobStore>) -> Self {
        Self { assets }
    }
}

impl ComponentLoader for BlobComponentLoader {
    fn load<'a>(
        &'a self,
        sha256: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>> {
        Box::pin(async move {
            self.assets
                .get(sha256, Some(EXT_BLOB_PROJECT))
                .await
                .map_err(|e| e.to_string())
        })
    }
}

/// Spawn the **event-reaction** loop: subscribe to the store's live event bus and
/// dispatch every durable [`LiveEvent::Transition`] to the enabled event-reaction
/// extensions, FAIL-OPEN and cycle-guarded (design §3.2/§3.4).
///
/// This runs in its **own** tokio task, entirely off the scheduler tick path, so a
/// reaction that faults, loops, or is killed by the resource regime can never wedge
/// a tick. A no-op when no dispatcher is wired.
pub fn spawn_event_reactions(store: StoreHandle, ext: &ExtHooks) {
    let Some(dispatcher) = ext.dispatcher().cloned() else {
        return;
    };
    tokio::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        let mut rx = store.subscribe();
        tracing::info!("extension event-reaction loop started (fail-open, cycle-guarded)");
        loop {
            match rx.recv().await {
                Ok(LiveEvent::Transition(event)) => {
                    let host_event =
                        HostEvent::transition(event.task, event.run, event.from, event.to);
                    // Fail-open: dispatch never blocks anything and catches every
                    // guest fault internally; nothing here can touch a tick.
                    dispatcher.dispatch_event(host_event).await;
                }
                // Other live events (activity, chat, hcom log) aren't reaction
                // triggers in v1.
                Ok(_) => {}
                // A slow consumer dropped some events — fine, transitions are also
                // durable in the store; keep going.
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "event-reaction loop lagged; continuing");
                }
                // The bus closed (store dropped) — the daemon is shutting down.
                Err(RecvError::Closed) => break,
            }
        }
    });
}
