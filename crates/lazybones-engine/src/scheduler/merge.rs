//! Land a green task branch back onto base and push (`fast-forward | merge | pr`).
//!
//! Runs after the gate passes. Returns the commit sha to record on `done`. For
//! `Pr` we only push the branch — opening the PR is left out of band (a human or
//! reviewer agent), per SCOPE.md open question 1.

use lazybones_store::Task;

use crate::config::{EngineConfig, MergeMode};

use super::git::git;

/// Merge `task`'s branch into base per `cfg.merge`, push, and return the merged
/// commit sha.
///
/// # Errors
/// Returns an error if any git step fails; the caller turns it into a `Block`.
pub async fn land(task: &Task, cfg: &EngineConfig) -> anyhow::Result<String> {
    let repo = &cfg.target_repo;
    let branch = task
        .branch
        .clone()
        .ok_or_else(|| anyhow::anyhow!("task {} has no branch to merge", task.id))?;

    match cfg.merge {
        MergeMode::Pr => {
            // Push the branch; a PR is opened out of band. Record the branch head.
            push(cfg, &branch).await?;
            head_sha(task, cfg, &branch).await
        }
        MergeMode::FastForward | MergeMode::Merge => {
            // Move onto base, integrate the branch, push base, then push the
            // branch too so the remote keeps the task ref.
            checkout(repo, &cfg.base_branch).await?;
            let args: Vec<&str> = match cfg.merge {
                MergeMode::FastForward => vec!["merge", "--ff-only", &branch],
                // A merge commit keeps history when base has moved under us.
                _ => vec!["merge", "--no-edit", &branch],
            };
            let out = git(repo, &args).await?;
            if !out.ok {
                anyhow::bail!("git merge of {branch} into {} failed: {}", cfg.base_branch, out.stderr);
            }
            push(cfg, &cfg.base_branch).await?;
            let sha = rev_parse(repo, "HEAD").await?;
            Ok(sha)
        }
    }
}

/// `git checkout <branch>` in the main repo.
async fn checkout(repo: &std::path::Path, branch: &str) -> anyhow::Result<()> {
    let out = git(repo, &["checkout", branch]).await?;
    if !out.ok {
        anyhow::bail!("git checkout {branch} failed: {}", out.stderr);
    }
    Ok(())
}

/// `git push <remote> <ref>`.
async fn push(cfg: &EngineConfig, refname: &str) -> anyhow::Result<()> {
    let out = git(&cfg.target_repo, &["push", &cfg.remote, refname]).await?;
    if !out.ok {
        anyhow::bail!("git push {} {refname} failed: {}", cfg.remote, out.stderr);
    }
    Ok(())
}

/// Resolve the branch head, preferring the task's worktree if it has one.
async fn head_sha(task: &Task, cfg: &EngineConfig, branch: &str) -> anyhow::Result<String> {
    let repo = task
        .worktree
        .as_deref()
        .map_or(cfg.target_repo.as_path(), std::path::Path::new);
    rev_parse(repo, branch).await
}

/// `git rev-parse <ref>` → the sha.
async fn rev_parse(repo: &std::path::Path, refname: &str) -> anyhow::Result<String> {
    let out = git(repo, &["rev-parse", refname]).await?;
    if !out.ok {
        anyhow::bail!("git rev-parse {refname} failed: {}", out.stderr);
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    async fn init_repo(dir: &Path) {
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
        ] {
            git(dir, &args).await.unwrap();
        }
        std::fs::write(dir.join("README.md"), "x").unwrap();
        git(dir, &["add", "."]).await.unwrap();
        git(dir, &["commit", "-m", "init"]).await.unwrap();
    }

    fn cfg_for(repo: &Path, merge: MergeMode) -> EngineConfig {
        EngineConfig {
            target_repo: repo.to_path_buf(),
            base_branch: "main".into(),
            remote: "origin".into(),
            gate: vec![],
            concurrency: 3,
            worktrees: true,
            worktree_root: ".lazy/wt".into(),
            branch_prefix: "lazy/".into(),
            merge,
            agent_tool: "claude".into(),
            stale_after_secs: 300,
            tick_secs: 2,
        }
    }

    #[tokio::test]
    async fn fast_forward_merges_branch_into_base() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        // A task branch with one commit ahead of main.
        git(dir.path(), &["checkout", "-b", "lazy/auth"]).await.unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        git(dir.path(), &["add", "."]).await.unwrap();
        git(dir.path(), &["commit", "-m", "work"]).await.unwrap();
        let want = rev_parse(dir.path(), "HEAD").await.unwrap();

        let cfg = cfg_for(dir.path(), MergeMode::FastForward);
        let mut task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        task.branch = Some("lazy/auth".into());
        // No remote configured, so the push step fails — assert the merge itself
        // landed by checking base before the push error.
        let _ = land(&task, &cfg).await;
        let base = rev_parse(dir.path(), "main").await.unwrap();
        assert_eq!(base, want, "main should fast-forward to the branch head");
    }
}
