//! A git-backed **sync repo**: the dumb transport that carries the content-sync
//! file tree (`lazybones_store::export_all`) between machines.
//!
//! This is the "simple sync" half of the long-term plan in
//! `docs/lazybones-server/`: before the cloud team plane exists, a developer who
//! works on PC-1 and then PC-2 just needs their authored docs/skills/tasks/
//! templates/workflows to follow them. A plain git repo in the cloud is enough —
//! PC-1 exports + commits + pushes "before it leaves", PC-2 pulls + imports "on
//! boot". Last-writer-wins, no merge logic, because it's one operator at a time.
//!
//! Why git via the CLI (not a git library or the GitHub API): same reason the
//! rest of this crate shells out to `gh`/`git` — the user's existing credentials
//! (`gh auth login`, SSH keys, credential helpers) already work, so there's *no
//! token handling here*. A [`SyncRepo`] is just a typed driver over `git -C
//! <dir>` against a chosen `branch`.
//!
//! ```no_run
//! # async fn demo() -> Result<(), lazybones_gh::GhError> {
//! let repo = lazybones_gh::SyncRepo::clone(
//!     "git@github.com:me/lazybones-sync.git",
//!     "/home/me/.lazybones/sync",
//!     "main",
//! ).await?;
//! repo.pull().await?;                       // catch up on boot → then import
//! // ... export the store into repo.dir() ...
//! repo.commit_and_push("sync 2026-06-25").await?;  // before you leave
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};

use crate::error::GhError;
use crate::Gh;

/// A local checkout of the sync repo, pinned to one `branch`. Cheap to clone.
#[derive(Debug, Clone)]
pub struct SyncRepo {
    gh: Gh,
    dir: PathBuf,
    branch: String,
}

/// The outcome of a [`SyncRepo::commit_and_push`] — whether there was anything to
/// send. `Clean` means the working tree matched `HEAD`, so nothing was committed
/// or pushed (a no-op sync, the common case when nothing changed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pushed {
    /// A commit was made and pushed to the remote.
    Committed,
    /// Nothing changed; no commit and no push.
    Clean,
}

impl SyncRepo {
    /// Open an existing local checkout at `dir`, treating `branch` as the sync
    /// branch. Does not touch the network — pair with [`pull`](Self::pull). Use
    /// [`clone`](Self::clone) for a first-time checkout, or [`init`](Self::init)
    /// for a local-only repo with no remote yet.
    #[must_use]
    pub fn open(dir: impl Into<PathBuf>, branch: impl Into<String>) -> Self {
        Self {
            gh: Gh::new(),
            dir: dir.into(),
            branch: branch.into(),
        }
    }

    /// Use a specific `git`-capable `gh` binary (tests, pinned installs).
    #[must_use]
    pub fn with_gh(mut self, gh: Gh) -> Self {
        self.gh = gh;
        self
    }

    /// The local checkout directory — where the store's export tree is read from
    /// and written to.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The branch this repo syncs on.
    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Whether `dir` is already a git checkout (has a `.git`). Lets a caller pick
    /// [`clone`](Self::clone) vs [`open`](Self::open)+[`pull`](Self::pull).
    #[must_use]
    pub fn is_checked_out(&self) -> bool {
        self.dir.join(".git").exists()
    }

    /// Clone `remote_url` into `dir` on `branch` and return the opened repo. The
    /// parent of `dir` is created if missing. For a brand-new (empty) remote with
    /// no `branch` yet, prefer [`init`](Self::init) + [`set_remote`](Self::set_remote).
    ///
    /// # Errors
    /// Returns [`GhError`] if the parent can't be created or `git clone` fails
    /// (bad url, auth, or a non-existent `branch`).
    pub async fn clone(
        remote_url: &str,
        dir: impl Into<PathBuf>,
        branch: impl Into<String>,
    ) -> Result<Self, GhError> {
        let repo = Self::open(dir, branch);
        let parent = repo.parent()?;
        std::fs::create_dir_all(parent).map_err(|source| GhError::Spawn {
            bin: format!("create_dir_all {}", parent.display()),
            source,
        })?;
        repo.gh
            .git(
                parent,
                [
                    "clone",
                    "--branch",
                    &repo.branch,
                    "--single-branch",
                    remote_url,
                    &repo.dir.to_string_lossy(),
                ],
            )
            .await?;
        Ok(repo)
    }

    /// Initialise `dir` as a fresh git repo on `branch` (local-only, no remote).
    /// The first-PC / offline-backup path; attach a remote later with
    /// [`set_remote`](Self::set_remote). Idempotent: a no-op if already a checkout.
    ///
    /// # Errors
    /// Returns [`GhError`] if the directory can't be created or `git init` fails.
    pub async fn init(dir: impl Into<PathBuf>, branch: impl Into<String>) -> Result<Self, GhError> {
        let repo = Self::open(dir, branch);
        if repo.is_checked_out() {
            return Ok(repo);
        }
        std::fs::create_dir_all(&repo.dir).map_err(|source| GhError::Spawn {
            bin: format!("create_dir_all {}", repo.dir.display()),
            source,
        })?;
        repo.gh
            .git(&repo.dir, ["init", "-b", &repo.branch])
            .await?;
        Ok(repo)
    }

    /// Configure git to use `gh`'s stored token as the credential helper for its
    /// known hosts (`gh auth setup-git`). This is what lets an `https://…` remote
    /// push/pull with the auth the user set up via `gh auth login` — no SSH keys
    /// required. Idempotent and global (git config); call it before clone/push.
    ///
    /// # Errors
    /// Returns [`GhError`] if `gh` isn't installed or the setup command fails.
    pub async fn setup_git_auth(&self) -> Result<(), GhError> {
        self.gh.run(".", ["auth", "setup-git"]).await.map(|_| ())
    }

    /// Point the checkout at `remote_url` as `origin` (idempotent: replaces an
    /// existing `origin`). Lets an [`init`](Self::init)-ed local repo start
    /// pushing once the cloud remote exists.
    ///
    /// # Errors
    /// Returns [`GhError`] if the `git remote` calls fail.
    pub async fn set_remote(&self, remote_url: &str) -> Result<(), GhError> {
        // `set-url` if it exists, else `add`. Probe first so this is idempotent.
        let exists = self
            .gh
            .git(&self.dir, ["remote", "get-url", "origin"])
            .await
            .is_ok();
        let verb = if exists { "set-url" } else { "add" };
        self.gh
            .git(&self.dir, ["remote", verb, "origin", remote_url])
            .await
            .map(|_| ())
    }

    /// Fast-forward the checkout from `origin/<branch>` (`git pull --ff-only`).
    /// `--ff-only` makes a divergence (someone edited PC-2 without pulling) a
    /// loud, recoverable error rather than a silent merge commit — exactly the
    /// failure mode the sync model warns about. A no-op when already current.
    ///
    /// # Errors
    /// Returns [`GhError`] if the pull fails (network, auth, or non-fast-forward
    /// divergence).
    pub async fn pull(&self) -> Result<(), GhError> {
        self.gh
            .git(&self.dir, ["pull", "--ff-only", "origin", &self.branch])
            .await
            .map(|_| ())
    }

    /// Fetch `origin/<branch>` without touching the working tree (`git fetch`).
    /// The cheap network probe behind out-of-sync detection: fetch, then compare
    /// with [`ahead_behind`](Self::ahead_behind).
    ///
    /// # Errors
    /// Returns [`GhError`] if the fetch fails (network, auth).
    pub async fn fetch(&self) -> Result<(), GhError> {
        self.gh
            .git(&self.dir, ["fetch", "origin", &self.branch])
            .await
            .map(|_| ())
    }

    /// How far local `HEAD` is `(ahead, behind)` `origin/<branch>`, counting
    /// commits each side has that the other doesn't. Run [`fetch`](Self::fetch)
    /// first so the remote ref is current. `(0, 0)` means in sync; `behind > 0`
    /// means the remote has changes to pull; `ahead > 0` means local has commits
    /// to push; both non-zero means diverged.
    ///
    /// # Errors
    /// Returns [`GhError`] if the rev-list fails (e.g. `origin/<branch>` unknown
    /// because nothing has been fetched/pushed yet).
    pub async fn ahead_behind(&self) -> Result<(u32, u32), GhError> {
        // `--left-right --count A...B` prints "<left>\t<right>": commits reachable
        // from HEAD but not origin (ahead), then from origin but not HEAD (behind).
        let spec = format!("origin/{}...HEAD", self.branch);
        let out = self
            .gh
            .git(&self.dir, ["rev-list", "--left-right", "--count", &spec])
            .await?;
        let mut cols = out.split_whitespace();
        let behind = cols.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let ahead = cols.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        Ok((ahead, behind))
    }

    /// Whether the working tree has uncommitted changes (`git status --porcelain`
    /// is non-empty) — i.e. an export actually changed something.
    ///
    /// # Errors
    /// Returns [`GhError`] if `git status` fails.
    pub async fn is_dirty(&self) -> Result<bool, GhError> {
        let out = self
            .gh
            .git(&self.dir, ["status", "--porcelain"])
            .await?;
        Ok(!out.trim().is_empty())
    }

    /// Stage everything (`git add -A`, including deletions), commit with
    /// `message`, and push to `origin/<branch>`. Returns [`Pushed::Clean`] without
    /// committing or pushing when the tree is already clean, so calling it after
    /// an unchanged export is a cheap no-op.
    ///
    /// # Errors
    /// Returns [`GhError`] if any of `git add`/`commit`/`push` fail.
    pub async fn commit_and_push(&self, message: &str) -> Result<Pushed, GhError> {
        self.gh.git(&self.dir, ["add", "-A"]).await?;
        // After staging, "nothing to commit" means the export was a no-op. Detect
        // it with `diff --cached --quiet` (exit 0 = no staged changes) so we don't
        // create an empty commit or treat it as an error.
        let clean = self
            .gh
            .git(&self.dir, ["diff", "--cached", "--quiet"])
            .await
            .is_ok();
        if clean {
            return Ok(Pushed::Clean);
        }
        self.gh
            .git(&self.dir, ["commit", "-m", message])
            .await?;
        self.gh
            .git(&self.dir, ["push", "origin", &self.branch])
            .await?;
        Ok(Pushed::Committed)
    }

    /// The current `HEAD` commit sha (full). Handy for logging "synced at <sha>".
    ///
    /// # Errors
    /// Returns [`GhError`] if `git rev-parse` fails (e.g. no commits yet).
    pub async fn head(&self) -> Result<String, GhError> {
        self.gh.git(&self.dir, ["rev-parse", "HEAD"]).await
    }

    /// The parent directory the checkout lives in (clone target's container).
    fn parent(&self) -> Result<&Path, GhError> {
        self.dir.parent().ok_or_else(|| GhError::WorkingDir {
            bin: "git clone".into(),
            dir: self.dir.display().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configure identity on a fresh checkout so `git commit` works in CI/sandbox
    /// where no global git identity is set.
    async fn identify(repo: &SyncRepo) {
        repo.gh
            .git(repo.dir(), ["config", "user.email", "t@t"])
            .await
            .unwrap();
        repo.gh
            .git(repo.dir(), ["config", "user.name", "t"])
            .await
            .unwrap();
    }

    /// End-to-end: a bare "cloud" remote, PC-1 pushes a file, PC-2 clones + pulls
    /// it — the whole sync-repo round trip with no GitHub and no network.
    #[tokio::test]
    async fn push_from_one_clone_pull_on_another() {
        let gh = Gh::new();
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        gh.git(tmp.path(), ["init", "--bare", "-b", "main", remote.to_str().unwrap()])
            .await
            .unwrap();
        let remote_url = remote.to_string_lossy().into_owned();

        // PC-1: a fresh local repo seeds the (empty) remote — init + set_remote +
        // push, the genuine first-machine path. (You can't `clone --branch main`
        // an empty remote; that branch only exists once PC-1 pushes it.)
        let pc1 = SyncRepo::init(tmp.path().join("pc1"), "main").await.unwrap();
        identify(&pc1).await;
        pc1.set_remote(&remote_url).await.unwrap();
        std::fs::create_dir_all(pc1.dir().join("skills")).unwrap();
        std::fs::write(pc1.dir().join("skills/a.yaml"), "id: a\n").unwrap();
        assert!(pc1.is_dirty().await.unwrap());
        assert_eq!(
            pc1.commit_and_push("first sync").await.unwrap(),
            Pushed::Committed
        );
        // A second push with no changes is a clean no-op.
        assert_eq!(
            pc1.commit_and_push("nothing").await.unwrap(),
            Pushed::Clean
        );

        // PC-2: a fresh clone sees the file; pull is a no-op but must succeed.
        let pc2 = SyncRepo::clone(&remote_url, tmp.path().join("pc2"), "main")
            .await
            .unwrap();
        assert!(pc2.dir().join("skills/a.yaml").exists());
        pc2.pull().await.unwrap();
        assert!(!pc2.head().await.unwrap().is_empty());
    }

    /// `init` + `set_remote` is the first-PC path: a local repo gains an origin
    /// and can then push to a previously-empty remote.
    #[tokio::test]
    async fn init_then_set_remote_and_push() {
        let gh = Gh::new();
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        gh.git(tmp.path(), ["init", "--bare", "-b", "main", remote.to_str().unwrap()])
            .await
            .unwrap();

        let repo = SyncRepo::init(tmp.path().join("local"), "main").await.unwrap();
        assert!(repo.is_checked_out());
        identify(&repo).await;
        repo.set_remote(&remote.to_string_lossy()).await.unwrap();
        // set_remote is idempotent (covers both add and set-url).
        repo.set_remote(&remote.to_string_lossy()).await.unwrap();

        std::fs::write(repo.dir().join("README.md"), "hi\n").unwrap();
        assert_eq!(
            repo.commit_and_push("seed").await.unwrap(),
            Pushed::Committed
        );
    }

    #[tokio::test]
    async fn init_is_idempotent_on_existing_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("r");
        let first = SyncRepo::init(&dir, "main").await.unwrap();
        identify(&first).await;
        std::fs::write(dir.join("f"), "x").unwrap();
        first.gh.git(&dir, ["add", "-A"]).await.unwrap();
        first.gh.git(&dir, ["commit", "-m", "c"]).await.unwrap();
        // Re-init must not wipe the existing repo/commit.
        let again = SyncRepo::init(&dir, "main").await.unwrap();
        assert!(!again.head().await.unwrap().is_empty());
    }
}
