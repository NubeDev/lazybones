//! Run `git -C <repo> …` and capture its result — the shared shell-out used by
//! worktree provisioning, gating, and merging.

use std::path::Path;

/// The captured outcome of a git invocation.
pub struct GitOut {
    /// Whether git exited zero.
    pub ok: bool,
    /// Trimmed stdout.
    pub stdout: String,
    /// Trimmed stderr.
    pub stderr: String,
}

/// Run `git -C <repo> <args…>` and capture the outcome.
///
/// # Errors
/// Returns an error only if git cannot be launched at all (not on a non-zero
/// exit — that surfaces as `ok == false`).
pub async fn git(repo: &Path, args: &[&str]) -> anyhow::Result<GitOut> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await?;
    Ok(GitOut {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
    })
}
