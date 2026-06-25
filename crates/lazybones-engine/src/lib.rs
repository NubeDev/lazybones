//! The lazybones execution plane: the in-process scheduler + a typed hcom client.
//!
//! The loop is a Tokio task inside `lazybonesd` (not a shell script): it reads
//! ready tasks from the shared [`StoreHandle`], provisions worktrees, spawns
//! agents by invoking the `hcom` CLI, gates the result, and advances state. See
//! `docs/scheduler.md` for the implementation-grade spec.

mod config;
mod hcom;
pub mod management;
mod scheduler;
pub mod sync;

pub use config::{EngineConfig, MergeMode};
pub use management::{TurnContext, chat_turn, page_context_workflow_id, render_page_context};
pub use scheduler::ext::{BlobComponentLoader, ExtHooks};
pub use scheduler::issue::IssueError;
pub use scheduler::{run, run_with_ext, workspace_preflight};

use hcom::Hcom;
use lazybones_gh::Gh;
use lazybones_store::{StoreHandle, Task};

/// Create a GitHub issue from a task's title/spec and link it (control-surface
/// primitive behind `POST /tasks/:id/issue`). Uses the user's authenticated
/// `gh` CLI; the repo is resolved from the task's workflow.
///
/// # Errors
/// Returns [`IssueError`] for a standalone task, a missing/unauthed `gh`, or a
/// `gh`/store failure.
pub async fn issue_create(store: &StoreHandle, id: &str) -> Result<Task, IssueError> {
    scheduler::issue::create(store, &Gh::new(), id).await
}

/// Link an existing GitHub issue to a task by URL or `#number`, validating it
/// resolves (behind `POST /tasks/:id/issue/link`).
///
/// # Errors
/// Returns [`IssueError`] for a bad link, a standalone task, or a `gh`/store failure.
pub async fn issue_link(store: &StoreHandle, id: &str, link: &str) -> Result<Task, IssueError> {
    scheduler::issue::link(store, &Gh::new(), id, link).await
}

/// Unlink a task's GitHub issue (behind `DELETE /tasks/:id/issue`). Clears the
/// task's link only; never touches the GitHub issue itself.
///
/// # Errors
/// Returns [`IssueError`] if the task is missing or the store write fails.
pub async fn issue_unlink(store: &StoreHandle, id: &str) -> Result<Task, IssueError> {
    scheduler::issue::unlink(store, id).await
}

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
/// exists". We therefore also delete the branch locally (`branch -D`), prune stale
/// worktree admin entries, and — when `remote` is set — delete the branch on the
/// remote too (`push <remote> --delete <branch>`), so a re-Start (e.g. after a PR
/// branch was pushed) provisions from a clean base with no leftover commits.
///
/// All steps are `--force`/best-effort (a restart is a deliberate discard of
/// in-flight work) and non-fatal: a missing tree, absent branch, or a remote that
/// never had the branch must not fail the restart. Failures are logged.
///
/// # Errors
/// Returns an error only if git cannot be launched at all.
pub async fn remove_worktree(
    repo: &std::path::Path,
    worktree: &str,
    branch: Option<&str>,
    remote: Option<&str>,
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
        // The common failure here is a *stale admin record*: the directory exists
        // on disk but git no longer lists it as a registered worktree ("is not a
        // working tree"), so `worktree remove` refuses it. That state is fatal to
        // the rest of the reset — the branch is still considered checked out in
        // this orphaned dir, so the `branch -D` below would fail with "checked out
        // at ...". Recover by force: prune the admin records and delete the
        // directory off disk ourselves, then prune again. After this the path is
        // gone and the branch is no longer checked out anywhere, so `branch -D`
        // succeeds. Without it, a restart leaves both the tree and the branch.
        tracing::warn!(
            worktree = %worktree,
            "remove_worktree: git worktree remove failed; forcing prune + rmdir: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        let _ = run(vec!["worktree".into(), "prune".into()]).await;
        if let Err(e) = tokio::fs::remove_dir_all(worktree).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(worktree = %worktree, "remove_worktree: rmdir failed (continuing): {e}");
        }
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

        // Delete the branch on the remote too, so a re-run after a PR-push starts
        // from a clean base. Best-effort: a remote that never had the branch (never
        // pushed) or no such remote returns non-zero — logged, not fatal. We probe
        // the remote first so a purely-local repo (no `origin`) is silently skipped.
        if let Some(remote) = remote {
            let has_remote = run(vec!["remote".into(), "get-url".into(), remote.into()])
                .await
                .map(|o| o.status.success())
                .unwrap_or(false);
            if has_remote {
                let out = run(vec![
                    "push".into(),
                    remote.into(),
                    "--delete".into(),
                    branch.into(),
                ])
                .await?;
                if !out.status.success() {
                    tracing::warn!(
                        branch = %branch,
                        remote = %remote,
                        "remove_worktree: git push --delete failed (continuing): {}",
                        String::from_utf8_lossy(&out.stderr).trim()
                    );
                }
            }
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
        /// Per-engine drive-tracking set, mirroring what the supervisor owns so a
        /// test tick re-attaches/claims through the same guard the daemon uses.
        driving: crate::scheduler::finish::Driving,
    }

    impl Engine {
        /// Build an engine whose hcom client invokes `bin` (a test stub path).
        #[must_use]
        pub fn with_hcom_bin(store: StoreHandle, cfg: EngineConfig, bin: &str) -> Self {
            Self {
                hcom: Hcom::discover().with_bin(bin),
                store,
                cfg,
                driving: crate::scheduler::finish::Driving::default(),
            }
        }

        /// Run one scheduler tick (reconcile → recover → promote → claim → spawn).
        ///
        /// Passes a non-due tick counter so the coarse-cadence reverse issue-sync
        /// (which would shell out to `gh`) never fires in a single-tick test;
        /// tests that exercise the sync call [`tick_n`](Self::tick_n) directly.
        ///
        /// Extensions are not wired in the test harness ([`ExtHooks::none`]), so a
        /// tick behaves exactly as the extension-free daemon.
        pub async fn tick(&self) {
            crate::scheduler::tick(
                &self.store,
                &self.hcom,
                &self.cfg,
                1,
                &self.driving,
                &crate::ExtHooks::none(),
            )
            .await;
        }

        /// Run one tick with an explicit `tick_count` — lets a test drive the
        /// Nth-tick reverse issue-sync deterministically.
        pub async fn tick_n(&self, tick_count: u64) {
            crate::scheduler::tick(
                &self.store,
                &self.hcom,
                &self.cfg,
                tick_count,
                &self.driving,
                &crate::ExtHooks::none(),
            )
            .await;
        }
    }

    /// Convenience: run one tick against a freshly built engine.
    pub async fn run_once(store: StoreHandle, cfg: EngineConfig, hcom_bin: &str) {
        Engine::with_hcom_bin(store, cfg, hcom_bin).tick().await;
    }
}

#[cfg(test)]
mod remove_worktree_tests {
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-b", "main"]);
        git(dir, &["config", "user.email", "t@t"]);
        git(dir, &["config", "user.name", "t"]);
        std::fs::write(dir.join("README.md"), "x").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-m", "init"]);
    }

    /// A hard restart must delete the task branch on the remote too, not just
    /// locally — else a re-run after a PR-push sees the stale remote branch. With a
    /// real (local-bare) origin holding the branch, `remove_worktree` must remove it
    /// there.
    #[tokio::test]
    async fn deletes_branch_on_the_remote() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare", "-b", "main"]);
        let work = tempfile::tempdir().unwrap();
        init_repo(work.path());
        git(
            work.path(),
            &["remote", "add", "origin", &remote.path().to_string_lossy()],
        );
        git(work.path(), &["push", "origin", "main"]);

        // A task branch with a worktree, pushed to origin.
        let wt = work.path().join(".lazy/wt/task1");
        git(
            work.path(),
            &[
                "worktree",
                "add",
                wt.to_str().unwrap(),
                "-b",
                "lazy/task1",
                "main",
            ],
        );
        std::fs::write(wt.join("a.txt"), "a").unwrap();
        git(&wt, &["add", "."]);
        git(&wt, &["commit", "-m", "work"]);
        git(&wt, &["push", "origin", "lazy/task1"]);
        // Sanity: the remote has the branch before the reset.
        let before = Command::new("git")
            .arg("-C")
            .arg(work.path())
            .args(["ls-remote", "origin", "refs/heads/lazy/task1"])
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&before.stdout).trim().is_empty(),
            "remote should have the branch first"
        );

        super::remove_worktree(
            work.path(),
            wt.to_str().unwrap(),
            Some("lazy/task1"),
            Some("origin"),
        )
        .await
        .unwrap();

        // Local branch gone...
        let local = Command::new("git")
            .arg("-C")
            .arg(work.path())
            .args(["show-ref", "--verify", "--quiet", "refs/heads/lazy/task1"])
            .output()
            .unwrap();
        assert!(!local.status.success(), "local branch must be deleted");
        // ...and the remote branch gone too.
        let after = Command::new("git")
            .arg("-C")
            .arg(work.path())
            .args(["ls-remote", "origin", "refs/heads/lazy/task1"])
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&after.stdout).trim().is_empty(),
            "remote branch must be deleted on a hard reset"
        );
    }

    /// No remote configured (purely local repo): the remote-delete step is silently
    /// skipped and the local branch is still removed — a restart must not fail just
    /// because there's no `origin`.
    #[tokio::test]
    async fn local_only_repo_skips_remote_delete() {
        let work = tempfile::tempdir().unwrap();
        init_repo(work.path());
        let wt = work.path().join(".lazy/wt/task1");
        git(
            work.path(),
            &[
                "worktree",
                "add",
                wt.to_str().unwrap(),
                "-b",
                "lazy/task1",
                "main",
            ],
        );

        // Should succeed despite there being no `origin` to delete from.
        super::remove_worktree(
            work.path(),
            wt.to_str().unwrap(),
            Some("lazy/task1"),
            Some("origin"),
        )
        .await
        .unwrap();

        let local = Command::new("git")
            .arg("-C")
            .arg(work.path())
            .args(["show-ref", "--verify", "--quiet", "refs/heads/lazy/task1"])
            .output()
            .unwrap();
        assert!(
            !local.status.success(),
            "local branch removed even with no remote"
        );
    }

    /// THE production failure on `simple-demo`: the worktree directory exists on
    /// disk but git's admin record for it is *stale* (pruned), so `git worktree
    /// remove` refuses it ("is not a working tree") AND the branch is still
    /// considered checked out there, so a naive `branch -D` fails ("checked out
    /// at ..."). `remove_worktree` must recover by force — prune + rmdir — and end
    /// with BOTH the directory and the local branch gone. Before the fix, a hard
    /// restart left both behind.
    #[tokio::test]
    async fn recovers_from_stale_worktree_admin_record() {
        let work = tempfile::tempdir().unwrap();
        init_repo(work.path());
        let wt = work.path().join(".lazy/wt/simple-demo");
        git(
            work.path(),
            &["worktree", "add", wt.to_str().unwrap(), "-b", "lazy/simple-demo", "main"],
        );
        // Reproduce the production state: corrupt the admin record's `gitdir`
        // backlink so it points nowhere. `git worktree list` still lists the tree
        // (as `prunable`), but `git worktree remove` then fails with the exact
        // error seen on `simple-demo`: "is not a working tree". The directory (with
        // its checked-out branch) stays on disk.
        let gitdir = work.path().join(".git/worktrees/simple-demo/gitdir");
        std::fs::write(&gitdir, "/tmp/lazybones-nonexistent/.git\n").unwrap();
        assert!(wt.is_dir(), "dir still on disk");
        // Sanity: the naive `worktree remove` now fails (the bug's trigger).
        let naive = Command::new("git")
            .arg("-C").arg(work.path())
            .args(["worktree", "remove", "--force", wt.to_str().unwrap()])
            .output().unwrap();
        assert!(!naive.status.success(), "naive worktree remove should fail on the corrupt record");

        // The real call must still fully clean up.
        super::remove_worktree(
            work.path(),
            wt.to_str().unwrap(),
            Some("lazy/simple-demo"),
            None,
        )
        .await
        .unwrap();

        assert!(!wt.exists(), "orphaned worktree dir must be removed");
        let local = Command::new("git")
            .arg("-C").arg(work.path())
            .args(["show-ref", "--verify", "--quiet", "refs/heads/lazy/simple-demo"])
            .output().unwrap();
        assert!(!local.status.success(), "local branch must be deleted after recovery");
    }
}
