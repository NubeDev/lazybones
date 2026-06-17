//! The lazybones execution plane: the in-process scheduler + a typed hcom client.
//!
//! The loop is a Tokio task inside `lazybonesd` (not a shell script): it reads
//! ready tasks from the shared [`StoreHandle`], provisions worktrees, spawns
//! agents by invoking the `hcom` CLI, gates the result, and advances state. See
//! `docs/scheduler.md` for the implementation-grade spec.

mod config;
mod hcom;
mod scheduler;

pub use config::{EngineConfig, MergeMode};
pub use scheduler::run;

use hcom::Hcom;

/// Cancel every hcom agent tagged with `tag` (a task id) — the control-surface
/// primitive behind `POST /tasks/:id/cancel`. Pairs with a `Block` transition
/// in the store; this half just stops the live agent.
///
/// # Errors
/// Returns an error if hcom cannot be launched or the kill exits non-zero.
pub async fn cancel_agent(tag: &str) -> anyhow::Result<()> {
    Hcom::discover().kill_tag(tag).await
}

#[doc(hidden)]
pub use harness::{Engine, run_once};

/// A thin public surface for integration tests: build the hcom client and run
/// single ticks deterministically (the production loop runs ticks forever).
///
/// `#[doc(hidden)]` — this is a test seam, not part of the daemon's API.
#[doc(hidden)]
pub mod harness {
    use lazybones_store::StoreHandle;

    use crate::EngineConfig;
    use crate::hcom::Hcom;

    /// A test handle bundling the store, hcom client, and config so a test can
    /// drive the scheduler one tick at a time.
    pub struct Engine {
        store: StoreHandle,
        hcom: Hcom,
        cfg: EngineConfig,
    }

    impl Engine {
        /// Build an engine whose hcom client invokes `bin` (a test stub path).
        #[must_use]
        pub fn with_hcom_bin(store: StoreHandle, cfg: EngineConfig, bin: &str) -> Self {
            Self {
                hcom: Hcom::discover().with_bin(bin),
                store,
                cfg,
            }
        }

        /// Run one scheduler tick (reconcile → promote → claim → spawn).
        pub async fn tick(&self) {
            crate::scheduler::tick(&self.store, &self.hcom, &self.cfg).await;
        }
    }

    /// Convenience: run one tick against a freshly built engine.
    pub async fn run_once(store: StoreHandle, cfg: EngineConfig, hcom_bin: &str) {
        Engine::with_hcom_bin(store, cfg, hcom_bin).tick().await;
    }
}
