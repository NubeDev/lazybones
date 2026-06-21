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

/// Post `text` to a task's hcom thread — the control-surface primitive behind
/// `POST /tasks/:id/chat` when the task is *running*. The agent listens on the
/// thread named after its task id, so this reaches it live; the durable record of
/// the message is written by the route into the `chat` store separately.
///
/// # Errors
/// Returns an error if hcom cannot be launched or the send exits non-zero.
pub async fn send_to_agent(thread: &str, text: &str) -> anyhow::Result<()> {
    Hcom::discover().send(thread, text).await
}

/// Tear down a task's git worktree at `worktree` (and, when `branch` is set, the
/// branch it was on) from `repo` — the control-surface primitive behind a
/// workflow *restart* with worktree teardown. Takes the repo/path/branch directly
/// (the route has them from the workspace + the task) rather than resolving an
/// `EffectiveGit`.
///
/// Removing the worktree alone is **not enough** for a clean restart: `git
/// worktree remove` leaves the branch behind, so the next `New`-mode claim's
/// `git worktree add -b <branch>` fails with "a branch named '<branch>' already
/// exists". We therefore also delete the branch (`branch -D`) and prune stale
/// worktree admin entries, so a re-Start provisions from scratch.
///
/// All steps are `--force`/best-effort (a restart is a deliberate discard of
/// in-flight work) and non-fatal: a missing tree or absent branch must not fail
/// the restart. Failures are logged.
///
/// # Errors
/// Returns an error only if git cannot be launched at all.
pub async fn remove_worktree(
    repo: &std::path::Path,
    worktree: &str,
    branch: Option<&str>,
) -> anyhow::Result<()> {
    let run = |args: Vec<String>| {
        let repo = repo.to_path_buf();
        async move {
            tokio::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(&args)
                .output()
                .await
        }
    };

    let out = run(vec![
        "worktree".into(),
        "remove".into(),
        "--force".into(),
        worktree.into(),
    ])
    .await?;
    if !out.status.success() {
        tracing::warn!(
            worktree = %worktree,
            "remove_worktree: git worktree remove failed (continuing): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    // Drop the admin record for any tree that's already gone from disk, so a
    // re-add at the same path doesn't trip "already registered".
    let _ = run(vec!["worktree".into(), "prune".into()]).await;

    // Delete the branch the tree was on — without this the next `worktree add
    // -b <branch>` collides. `-D` (force) since a restart discards its commits.
    if let Some(branch) = branch {
        let out = run(vec!["branch".into(), "-D".into(), branch.into()]).await?;
        if !out.status.success() {
            tracing::warn!(
                branch = %branch,
                "remove_worktree: git branch -D failed (continuing): {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
    }
    Ok(())
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
