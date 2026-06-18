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
pub use repo::{Branch, RepoView};

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
