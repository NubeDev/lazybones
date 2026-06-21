//! A thin async wrapper around the GitHub CLI (`gh`).
//!
//! Design choice: we deliberately shell out to the user's already-installed and
//! already-authenticated `gh` binary instead of talking to the GitHub REST API
//! directly. That means **no token handling here** — `gh` carries the auth that
//! the user set up once with `gh auth login`, including SSO, keyring storage and
//! token refresh. We just borrow it.
//!
//! For anything we don't model yet, [`Gh::run`] / [`Gh::run_json`] give raw
//! access to the CLI so callers (workflows, tasks) aren't blocked on this crate
//! growing a method first.
//!
//! ```no_run
//! # async fn demo() -> Result<(), lazybones_gh::GhError> {
//! let gh = lazybones_gh::Gh::new();
//! gh.ensure_auth().await?;                          // reuse existing login
//! let branches = gh.branches(".").await?;           // list branches
//! let issues = gh.issues(".", lazybones_gh::IssueState::Open).await?;
//! # Ok(())
//! # }
//! ```

mod error;
mod issue;
mod repo;

pub use error::GhError;
pub use issue::{Issue, IssueState};
pub use repo::{
    Branch, ChangeKind, LocalBranch, RepoView, TreeEntry, Worktree, resolve_status,
};

use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;

use serde::de::DeserializeOwned;
use tokio::process::Command;

/// Handle to the `gh` CLI. Cheap to clone; holds only the binary name.
#[derive(Debug, Clone)]
pub struct Gh {
    bin: String,
}

impl Default for Gh {
    fn default() -> Self {
        Self::new()
    }
}

impl Gh {
    /// Use `gh` from `PATH`.
    pub fn new() -> Self {
        Self { bin: "gh".into() }
    }

    /// Use a specific `gh` binary (tests, pinned installs).
    pub fn with_bin(bin: impl Into<String>) -> Self {
        Self { bin: bin.into() }
    }

    /// Run `gh <args...>` and return captured stdout (trimmed of trailing
    /// newline). `dir` is the working directory — pass the repo path so `gh`
    /// resolves the right remote; `.` for the current dir.
    pub async fn run<I, S>(&self, dir: impl AsRef<Path>, args: I) -> Result<String, GhError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let bin = self.bin.clone();
        exec(&bin, dir, args).await
    }

    /// Run `gh <args...>` and deserialize its stdout as JSON. Most `gh`
    /// subcommands emit JSON when given `--json <fields>`.
    pub async fn run_json<T, I, S>(&self, dir: impl AsRef<Path>, args: I) -> Result<T, GhError>
    where
        T: DeserializeOwned,
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let out = self.run(dir, args).await?;
        Ok(serde_json::from_str(&out)?)
    }

    /// Verify the user is logged in (`gh auth status`). Cheap pre-flight so a
    /// workflow fails with a clear "run `gh auth login`" instead of a confusing
    /// per-command error later.
    pub async fn ensure_auth(&self) -> Result<(), GhError> {
        self.run(".", ["auth", "status"]).await.map(|_| ())
    }

    // ---- repo / branches ------------------------------------------------

    /// `gh repo view` for the repo at `dir` (owner, name, default branch).
    pub async fn repo_view(&self, dir: impl AsRef<Path>) -> Result<RepoView, GhError> {
        self.run_json(
            dir,
            [
                "repo",
                "view",
                "--json",
                "name,owner,defaultBranchRef,url,description",
            ],
        )
        .await
    }

    /// List branches via the API endpoint (works without cloning the repo).
    pub async fn branches(&self, dir: impl AsRef<Path>) -> Result<Vec<Branch>, GhError> {
        // `--jq` with a streaming filter emits one JSON object per line rather
        // than a single array, so we parse line-by-line.
        let out = self
            .run(
                dir,
                [
                    "api",
                    "repos/{owner}/{repo}/branches",
                    "--paginate",
                    "--jq",
                    ".[] | {name, sha: .commit.sha, protected}",
                ],
            )
            .await?;

        out.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(GhError::Json))
            .collect()
    }

    /// List **local** branches via `git for-each-ref` — works with no GitHub
    /// remote, offline, and without `gh` auth (unlike [`branches`](Self::branches),
    /// which hits the GitHub API and needs `{owner}/{repo}` to expand). Each
    /// branch carries its short SHA and, when it tracks an upstream, the
    /// ahead/behind counts. This is what a *local* repo manager should use.
    pub async fn branches_local(
        &self,
        dir: impl AsRef<Path>,
    ) -> Result<Vec<LocalBranch>, GhError> {
        // Tab-delimited so branch names (which can't contain a tab) parse
        // unambiguously. `%(upstream:track,nobracket)` is the safe way to get
        // ahead/behind: it prints "ahead N, behind M" when an upstream exists
        // and an empty string otherwise — never aborting (unlike
        // `%(ahead-behind:...)`, which is fatal for branches with no upstream).
        let out = self
            .git(
                dir,
                [
                    "for-each-ref",
                    "--format=%(refname:short)\t%(objectname:short)\t%(upstream:short)\t%(upstream:track,nobracket)",
                    "refs/heads/",
                ],
            )
            .await?;
        Ok(out.lines().filter_map(LocalBranch::parse_line).collect())
    }

    /// Create a new local branch in the repo at `dir` and check it out.
    ///
    /// This is a *local git* operation, not a `gh` one — `gh` has no
    /// branch-create command — so we shell out to `git` directly. Kept here so
    /// callers get the whole "pick a repo, pick/make a branch" story from one
    /// place. `from` is the start point (a branch/sha); `None` branches off the
    /// current `HEAD`.
    pub async fn create_branch(
        &self,
        dir: impl AsRef<Path>,
        name: &str,
        from: Option<&str>,
    ) -> Result<(), GhError> {
        let mut args = vec!["switch".to_string(), "-c".to_string(), name.to_string()];
        if let Some(start) = from {
            args.push(start.to_string());
        }
        self.git(dir, args).await.map(|_| ())
    }

    /// The current checked-out branch of the repo at `dir`.
    pub async fn current_branch(&self, dir: impl AsRef<Path>) -> Result<String, GhError> {
        self.git(dir, ["rev-parse", "--abbrev-ref", "HEAD"]).await
    }

    /// Switch the repo at `dir` to an existing `branch` (`git checkout`).
    pub async fn checkout(&self, dir: impl AsRef<Path>, branch: &str) -> Result<(), GhError> {
        self.git(dir, ["checkout", branch]).await.map(|_| ())
    }

    /// Delete a local branch. `force` uses `-D` (delete even if unmerged);
    /// otherwise `-d` (refuses to drop unmerged work).
    pub async fn delete_branch(
        &self,
        dir: impl AsRef<Path>,
        name: &str,
        force: bool,
    ) -> Result<(), GhError> {
        let flag = if force { "-D" } else { "-d" };
        self.git(dir, ["branch", flag, name]).await.map(|_| ())
    }

    // ---- files / diff ---------------------------------------------------

    /// List the tracked + untracked (non-ignored) files of the repo at `dir`,
    /// as the immediate children of the repo-relative directory `rel` (`""` for
    /// the root). Backed by `git ls-files`, so it honours `.gitignore` and never
    /// descends into `.git`. Includes untracked-but-not-ignored files so the
    /// browser reflects the working tree as it actually is (agents create files
    /// before committing). Returns child dirs and files of `rel`, dirs first.
    pub async fn list_files(
        &self,
        dir: impl AsRef<Path>,
        rel: &str,
    ) -> Result<Vec<TreeEntry>, GhError> {
        // `-z` => NUL-delimited, robust to spaces/newlines in filenames.
        let out = self
            .git(
                dir,
                ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
            )
            .await?;
        Ok(TreeEntry::from_ls_files(&out, rel))
    }

    /// Build the *whole* repo file tree at `dir` (every dir + file, honouring
    /// `.gitignore`, including untracked-non-ignored), paired with a git-status
    /// map for decorating changed paths. With `base = None` the status is the
    /// uncommitted working-tree changes; with `base = Some("master")` it's also
    /// folded with what the current branch changed vs that base. This is what
    /// the VSCode-style file tree renders in one shot.
    pub async fn tree(
        &self,
        dir: impl AsRef<Path>,
        base: Option<&str>,
    ) -> Result<(Vec<TreeEntry>, std::collections::BTreeMap<String, repo::ChangeKind>), GhError>
    {
        let dir = dir.as_ref();
        let listed = self
            .git(
                dir,
                ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
            )
            .await?;
        let entries = TreeEntry::full_tree(&listed);

        // Uncommitted changes (always shown).
        let status_out = self
            .git(dir, ["status", "--porcelain=v1", "-z"])
            .await?;
        let mut status = repo::parse_status(&status_out);

        // Branch-vs-base, when a base is given: union it in (uncommitted wins
        // on conflict, since it's the more "live" signal).
        if let Some(base) = base {
            let ns = self
                .git(dir, ["diff", "--name-status", "-z", &format!("{base}...")])
                .await?;
            for (path, kind) in repo::parse_name_status(&ns) {
                status.entry(path).or_insert(kind);
            }
        }

        Ok((entries, status))
    }

    /// Read a repo-relative file at `rel` from the working tree of the repo at
    /// `dir`. Reads off disk (not `git show`) so it reflects uncommitted edits;
    /// returns the bytes lossily as UTF-8. The caller guards `rel` against
    /// escaping the repo.
    pub async fn read_file(
        &self,
        dir: impl AsRef<Path>,
        rel: &str,
    ) -> Result<String, GhError> {
        let path = dir.as_ref().join(rel);
        // A single small file read; std::fs keeps us off the `tokio/fs` feature
        // (not enabled workspace-wide) and matches the engine's fs_list path.
        let bytes = std::fs::read(&path).map_err(|source| GhError::Spawn {
            bin: format!("read {}", path.display()),
            source,
        })?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Diff the working tree of the repo at `dir`. `base = None` shows the
    /// *uncommitted* changes (`git diff HEAD`, staged + unstaged). `base =
    /// Some("master")` shows everything the current branch changed relative to
    /// that base via the merge-base (`git diff base...`), the "what did this
    /// workflow do" view. `rel`, when set, scopes the diff to one path.
    pub async fn diff(
        &self,
        dir: impl AsRef<Path>,
        base: Option<&str>,
        rel: Option<&str>,
    ) -> Result<String, GhError> {
        let mut args = vec!["diff".to_string()];
        match base {
            // Three-dot (`base...`) diffs against the merge-base, so commits
            // that landed on base *after* we branched don't pollute the view.
            Some(b) => args.push(format!("{b}...")),
            None => args.push("HEAD".to_string()),
        }
        if let Some(rel) = rel {
            args.push("--".to_string());
            args.push(rel.to_string());
        }
        self.git(dir, args).await
    }

    // ---- worktrees ------------------------------------------------------

    /// List the repo's worktrees (`git worktree list --porcelain`).
    pub async fn worktrees(&self, dir: impl AsRef<Path>) -> Result<Vec<Worktree>, GhError> {
        let out = self
            .git(dir, ["worktree", "list", "--porcelain"])
            .await?;
        Ok(Worktree::parse_list(&out))
    }

    /// Remove a worktree by path (`git worktree remove`). `force` overrides the
    /// "contains modifications" / locked guards.
    pub async fn remove_worktree(
        &self,
        dir: impl AsRef<Path>,
        path: &str,
        force: bool,
    ) -> Result<(), GhError> {
        let mut args = vec!["worktree".to_string(), "remove".to_string()];
        if force {
            args.push("--force".to_string());
        }
        args.push(path.to_string());
        self.git(dir, args).await.map(|_| ())
    }

    /// Prune stale worktree administrative entries (`git worktree prune`).
    pub async fn prune_worktrees(&self, dir: impl AsRef<Path>) -> Result<(), GhError> {
        self.git(dir, ["worktree", "prune"]).await.map(|_| ())
    }

    /// Run `git <args...>` in `dir`. Shares [`run`](Self::run)'s spawn + error
    /// handling but targets the `git` binary (local repo operations `gh` can't do).
    pub async fn git<I, S>(&self, dir: impl AsRef<Path>, args: I) -> Result<String, GhError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        exec("git", dir, args).await
    }

    // ---- issues ---------------------------------------------------------

    /// List issues filtered by state.
    pub async fn issues(
        &self,
        dir: impl AsRef<Path>,
        state: IssueState,
    ) -> Result<Vec<Issue>, GhError> {
        self.run_json(
            dir,
            [
                "issue",
                "list",
                "--state",
                state.as_arg(),
                "--json",
                "number,title,state,url,author,labels,body",
            ],
        )
        .await
    }

    /// View one issue by number.
    pub async fn issue_view(
        &self,
        dir: impl AsRef<Path>,
        number: u64,
    ) -> Result<Issue, GhError> {
        self.run_json(
            dir,
            [
                "issue".to_string(),
                "view".to_string(),
                number.to_string(),
                "--json".to_string(),
                "number,title,state,url,author,labels,body".to_string(),
            ],
        )
        .await
    }

    /// Open a new issue; returns its URL.
    pub async fn issue_create(
        &self,
        dir: impl AsRef<Path>,
        title: &str,
        body: &str,
    ) -> Result<String, GhError> {
        self.run(dir, ["issue", "create", "--title", title, "--body", body])
            .await
    }

    /// Close an issue.
    pub async fn issue_close(
        &self,
        dir: impl AsRef<Path>,
        number: u64,
    ) -> Result<(), GhError> {
        self.run(dir, ["issue", "close", &number.to_string()])
            .await
            .map(|_| ())
    }
}

/// Spawn `bin <args...>` in `dir`, capture stdout (trailing newlines trimmed),
/// and map a non-zero exit or spawn failure to [`GhError`]. Shared by the `gh`
/// and `git` paths so both get identical error handling.
async fn exec<I, S>(bin: &str, dir: impl AsRef<Path>, args: I) -> Result<String, GhError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect();

    tracing::debug!(bin, args = ?args, "running");

    let output = Command::new(bin)
        .current_dir(dir)
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|source| GhError::Spawn {
            bin: bin.to_string(),
            source,
        })?;

    if !output.status.success() {
        return Err(GhError::Command {
            args: args.join(" "),
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
        });
    }

    let mut stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    while stdout.ends_with('\n') || stdout.ends_with('\r') {
        stdout.pop();
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    /// Write a throwaway executable shell script that stands in for `gh`, so we
    /// can exercise spawn + stdout capture + JSON parsing without a network or a
    /// real GitHub login.
    fn fake_gh(dir: &Path, body: &str) -> String {
        let path = dir.join("gh");
        {
            // Fully close the handle before exec — an open write fd to the file
            // we're about to run trips ETXTBSY ("Text file busy").
            let mut f = std::fs::File::create(&path).unwrap();
            write!(f, "#!/bin/sh\n{body}\n").unwrap();
            f.flush().unwrap();
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn run_captures_trimmed_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = fake_gh(tmp.path(), "printf 'hello\\n'");
        let gh = Gh::with_bin(bin);
        assert_eq!(gh.run(".", ["whoami"]).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn nonzero_exit_surfaces_stderr() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = fake_gh(tmp.path(), "echo 'not logged in' 1>&2; exit 1");
        let gh = Gh::with_bin(bin);
        let err = gh.ensure_auth().await.unwrap_err();
        match err {
            GhError::Command { stderr, .. } => assert!(stderr.contains("not logged in")),
            other => panic!("expected Command error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_binary_is_spawn_error() {
        let gh = Gh::with_bin("/nonexistent/gh-binary");
        assert!(matches!(
            gh.ensure_auth().await,
            Err(GhError::Spawn { .. })
        ));
    }

    #[tokio::test]
    async fn issues_parse_from_json() {
        let tmp = tempfile::tempdir().unwrap();
        let json = r#"[{"number":7,"title":"bug","state":"OPEN","url":"u","body":"b","author":{"login":"me"},"labels":[{"name":"p1"}]}]"#;
        let bin = fake_gh(tmp.path(), &format!("printf '%s' '{json}'"));
        let gh = Gh::with_bin(bin);
        let issues = gh.issues(".", IssueState::Open).await.unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 7);
        assert_eq!(issues[0].labels[0].name, "p1");
    }

    #[tokio::test]
    async fn create_and_read_current_branch() {
        // A real temp git repo: prove the git-backed branch ops actually work.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let gh = Gh::new();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@t"],
            &["config", "user.name", "t"],
            &["commit", "--allow-empty", "-q", "-m", "root"],
        ] {
            gh.git(dir, args).await.unwrap();
        }
        gh.create_branch(dir, "feat/x", None).await.unwrap();
        assert_eq!(gh.current_branch(dir).await.unwrap(), "feat/x");
    }

    #[test]
    fn tree_levels_from_ls_files() {
        // NUL-delimited `git ls-files -z` output across nested dirs + a root file.
        let out = "Cargo.toml\0src/lib.rs\0src/repo.rs\0src/error.rs\0docs/a.md\0";

        // Root level: two dirs (collapsed), one file. Dirs first, then files.
        let root = TreeEntry::from_ls_files(out, "");
        assert_eq!(
            root.iter().map(|e| (&e.name, e.is_dir)).collect::<Vec<_>>(),
            vec![
                (&"docs".to_string(), true),
                (&"src".to_string(), true),
                (&"Cargo.toml".to_string(), false),
            ],
        );

        // Descend into `src`: three files, repo-relative paths preserved.
        let src = TreeEntry::from_ls_files(out, "src");
        assert!(src.iter().all(|e| !e.is_dir));
        assert_eq!(
            src.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            vec!["src/error.rs", "src/lib.rs", "src/repo.rs"],
        );

        // A trailing slash on `rel` is tolerated.
        assert_eq!(TreeEntry::from_ls_files(out, "src/"), src);
        // Unknown dir => empty.
        assert!(TreeEntry::from_ls_files(out, "nope").is_empty());
    }

    #[test]
    fn full_tree_is_depth_first_dirs_before_files() {
        let out = "Cargo.toml\0src/lib.rs\0src/repo.rs\0src/a/x.rs\0docs/a.md\0";
        let tree = TreeEntry::full_tree(out);
        let shape: Vec<(&str, bool)> =
            tree.iter().map(|e| (e.path.as_str(), e.is_dir)).collect();
        // docs/ before src/ (dirs lexicographic); each dir immediately precedes
        // its subtree; nested dir `src/a` before sibling files of `src`; the
        // root file `Cargo.toml` sorts after all root dirs.
        assert_eq!(
            shape,
            vec![
                ("docs", true),
                ("docs/a.md", false),
                ("src", true),
                ("src/a", true),
                ("src/a/x.rs", false),
                ("src/lib.rs", false),
                ("src/repo.rs", false),
                ("Cargo.toml", false),
            ],
        );
    }

    #[test]
    fn full_tree_treats_nested_checkout_as_leaf_dir() {
        // `git ls-files` emits a nested worktree/submodule with a trailing slash.
        let out = ".lazy/wt/test-be/\0src/lib.rs\0";
        let tree = TreeEntry::full_tree(out);
        // The nested checkout is a directory leaf named `test-be`, never an
        // empty-named "file"; its ancestors `.lazy` and `.lazy/wt` are dirs too.
        let nested = tree.iter().find(|e| e.path == ".lazy/wt/test-be").unwrap();
        assert!(nested.is_dir);
        assert_eq!(nested.name, "test-be");
        assert!(!tree.iter().any(|e| e.name.is_empty()));
        assert!(tree.iter().any(|e| e.path == ".lazy/wt" && e.is_dir));
    }

    #[test]
    fn resolve_status_tags_files_under_untracked_dir() {
        use lazybones_gh_changekind::*;
        // git collapses an untracked dir to `?? .lazy/`; files inside it appear
        // individually in the tree and must inherit the untracked tag.
        let out = "?? .lazy/\0 M src/lib.rs\0";
        let m = repo::parse_status(out);
        assert_eq!(repo::resolve_status(&m, "src/lib.rs"), Some(Modified));
        assert_eq!(repo::resolve_status(&m, ".lazy/wt/test-be"), Some(Untracked));
        assert_eq!(repo::resolve_status(&m, ".lazy"), Some(Untracked));
        assert_eq!(repo::resolve_status(&m, "README.md"), None);
    }

    #[test]
    fn status_porcelain_parses_kinds_and_renames() {
        use lazybones_gh_changekind::*;
        // ` M f` (modified), `A  n` (added), `?? u` (untracked), `R  old\0new`.
        let out = " M src/lib.rs\0A  added.rs\0?? new.txt\0R  old.rs\0new.rs\0";
        let m = repo::parse_status(out);
        assert_eq!(m.get("src/lib.rs"), Some(&Modified));
        assert_eq!(m.get("added.rs"), Some(&Added));
        assert_eq!(m.get("new.txt"), Some(&Untracked));
        // Rename keys the new path, not the old.
        assert_eq!(m.get("new.rs"), Some(&Modified));
        assert!(!m.contains_key("old.rs"));
    }

    #[test]
    fn name_status_parses_branch_diff() {
        use lazybones_gh_changekind::*;
        let out = "M\0src/lib.rs\0A\0added.rs\0D\0gone.rs\0R100\0old.rs\0new.rs\0";
        let m = repo::parse_name_status(out);
        assert_eq!(m.get("src/lib.rs"), Some(&Modified));
        assert_eq!(m.get("added.rs"), Some(&Added));
        assert_eq!(m.get("gone.rs"), Some(&Deleted));
        assert_eq!(m.get("new.rs"), Some(&Modified));
        assert!(!m.contains_key("old.rs"));
    }

    #[tokio::test]
    async fn tree_decorates_changed_paths() {
        use lazybones_gh_changekind::*;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let gh = Gh::new();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@t"],
            &["config", "user.name", "t"],
        ] {
            gh.git(dir, args).await.unwrap();
        }
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn main() {}\n").unwrap();
        gh.git(dir, ["add", "."]).await.unwrap();
        gh.git(dir, ["commit", "-q", "-m", "init"]).await.unwrap();

        // Modify a tracked file + add an untracked one.
        std::fs::write(dir.join("src/lib.rs"), "fn main() { /* x */ }\n").unwrap();
        std::fs::write(dir.join("fresh.txt"), "new\n").unwrap();

        let (entries, status) = gh.tree(dir, None).await.unwrap();
        // The synthesised `src` dir is present, plus both files.
        assert!(entries.iter().any(|e| e.path == "src" && e.is_dir));
        assert!(entries.iter().any(|e| e.path == "fresh.txt" && !e.is_dir));
        assert_eq!(status.get("src/lib.rs"), Some(&Modified));
        assert_eq!(status.get("fresh.txt"), Some(&Untracked));
    }

    /// Re-export `ChangeKind` variants unqualified for terse test assertions.
    mod lazybones_gh_changekind {
        pub use crate::repo::ChangeKind::*;
    }

    #[tokio::test]
    async fn list_files_and_diff_on_real_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let gh = Gh::new();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@t"],
            &["config", "user.name", "t"],
        ] {
            gh.git(dir, args).await.unwrap();
        }
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join("README.md"), "hi\n").unwrap();
        gh.git(dir, ["add", "."]).await.unwrap();
        gh.git(dir, ["commit", "-q", "-m", "init"]).await.unwrap();

        // Root listing: `src` dir + `README.md` file.
        let root = gh.list_files(dir, "").await.unwrap();
        assert_eq!(root[0].name, "src");
        assert!(root[0].is_dir);
        assert!(root.iter().any(|e| e.name == "README.md" && !e.is_dir));

        // An untracked, non-ignored file shows up (working-tree view).
        std::fs::write(dir.join("new.txt"), "fresh\n").unwrap();
        assert!(gh.list_files(dir, "").await.unwrap().iter().any(|e| e.name == "new.txt"));

        // Read a file back.
        assert_eq!(gh.read_file(dir, "README.md").await.unwrap(), "hi\n");

        // Uncommitted diff: edit a tracked file, expect it in `diff(None)`.
        std::fs::write(dir.join("src/lib.rs"), "fn main() { /* x */ }\n").unwrap();
        let d = gh.diff(dir, None, None).await.unwrap();
        assert!(d.contains("src/lib.rs"));
        assert!(d.contains("/* x */"));
    }

    #[test]
    fn worktree_porcelain_parses() {
        let out = "worktree /repo\nHEAD abc123\nbranch refs/heads/master\n\n\
                   worktree /repo/.lazy/wt/feat-x\nHEAD def456\nbranch refs/heads/feat/x\nlocked\n\n\
                   worktree /repo/detached\nHEAD 0099aa\ndetached\n";
        let trees = Worktree::parse_list(out);
        assert_eq!(trees.len(), 3);

        assert_eq!(trees[0].path, "/repo");
        assert_eq!(trees[0].branch.as_deref(), Some("master"));
        assert_eq!(trees[0].head.as_deref(), Some("abc123"));
        assert!(trees[0].is_main);
        assert!(!trees[0].locked);

        assert_eq!(trees[1].branch.as_deref(), Some("feat/x"));
        assert!(!trees[1].is_main);
        assert!(trees[1].locked);

        // Detached HEAD: no branch.
        assert_eq!(trees[2].branch, None);
        assert_eq!(trees[2].head.as_deref(), Some("0099aa"));
    }

    #[tokio::test]
    async fn worktree_add_list_remove() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let gh = Gh::new();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@t"],
            &["config", "user.name", "t"],
            &["commit", "--allow-empty", "-q", "-m", "root"],
        ] {
            gh.git(dir, args).await.unwrap();
        }
        let wt = dir.join("wt-extra");
        gh.git(dir, ["worktree", "add", "-q", "-b", "feat/wt", wt.to_str().unwrap()])
            .await
            .unwrap();

        let trees = gh.worktrees(dir).await.unwrap();
        assert_eq!(trees.len(), 2);
        assert!(trees[0].is_main);
        assert!(trees.iter().any(|w| w.branch.as_deref() == Some("feat/wt")));

        gh.remove_worktree(dir, wt.to_str().unwrap(), false)
            .await
            .unwrap();
        assert_eq!(gh.worktrees(dir).await.unwrap().len(), 1);
        gh.prune_worktrees(dir).await.unwrap();
    }

    #[tokio::test]
    async fn checkout_and_delete_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let gh = Gh::new();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@t"],
            &["config", "user.name", "t"],
            &["commit", "--allow-empty", "-q", "-m", "root"],
        ] {
            gh.git(dir, args).await.unwrap();
        }
        let base = gh.current_branch(dir).await.unwrap();
        gh.create_branch(dir, "feat/y", None).await.unwrap();
        gh.checkout(dir, &base).await.unwrap();
        assert_eq!(gh.current_branch(dir).await.unwrap(), base);
        // feat/y is merged-equal to base (empty), so a plain -d works.
        gh.delete_branch(dir, "feat/y", false).await.unwrap();
        let names: Vec<String> = gh
            .branches_local(dir)
            .await
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert!(!names.contains(&"feat/y".to_string()));
    }

    #[tokio::test]
    async fn branches_local_lists_without_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let gh = Gh::new();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "t@t"],
            &["config", "user.name", "t"],
            &["commit", "--allow-empty", "-q", "-m", "root"],
        ] {
            gh.git(dir, args).await.unwrap();
        }
        gh.create_branch(dir, "feat/x", None).await.unwrap();
        // No remote configured at all — the GitHub-API `branches()` would fail
        // here, but the local lister must still work.
        let branches = gh.branches_local(dir).await.unwrap();
        assert!(branches.iter().any(|b| b.name == "feat/x"));
        // Without an upstream, ahead/behind are zero and upstream is None.
        let feat = branches.iter().find(|b| b.name == "feat/x").unwrap();
        assert_eq!(feat.upstream, None);
        assert_eq!((feat.ahead, feat.behind), (0, 0));
        assert!(!feat.sha.is_empty());
    }

    #[test]
    fn local_branch_parses_with_and_without_upstream() {
        let tracked =
            LocalBranch::parse_line("master\t76c0afe\torigin/master\tahead 2, behind 1")
                .unwrap();
        assert_eq!(tracked.name, "master");
        assert_eq!(tracked.sha, "76c0afe");
        assert_eq!(tracked.upstream.as_deref(), Some("origin/master"));
        assert_eq!((tracked.ahead, tracked.behind), (2, 1));

        // ahead-only and behind-only forms.
        let ahead_only =
            LocalBranch::parse_line("a\tsha\torigin/a\tahead 3").unwrap();
        assert_eq!((ahead_only.ahead, ahead_only.behind), (3, 0));
        let behind_only =
            LocalBranch::parse_line("b\tsha\torigin/b\tbehind 4").unwrap();
        assert_eq!((behind_only.ahead, behind_only.behind), (0, 4));

        let untracked = LocalBranch::parse_line("feat/x\ta1b2c3d\t\t").unwrap();
        assert_eq!(untracked.upstream, None);
        assert_eq!((untracked.ahead, untracked.behind), (0, 0));

        assert!(LocalBranch::parse_line("").is_none());
    }

    #[tokio::test]
    async fn branches_parse_line_delimited() {
        let tmp = tempfile::tempdir().unwrap();
        let lines = r#"{"name":"main","sha":"abc","protected":true}
{"name":"dev","sha":"def","protected":false}"#;
        let bin = fake_gh(tmp.path(), &format!("cat <<'EOF'\n{lines}\nEOF"));
        let gh = Gh::with_bin(bin);
        let branches = gh.branches(".").await.unwrap();
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[1].name, "dev");
        assert!(branches[0].protected);
    }
}
