//! Land a green task branch back onto base and push (`fast-forward | merge | pr`).
//!
//! Runs after the gate passes. Returns the commit sha to record on `done`. For
//! `Pr` we only push the branch — opening the PR is left out of band (a human or
//! reviewer agent), per SCOPE.md open question 1.

use lazybones_store::Task;

use crate::config::{EngineConfig, MergeMode};

use super::effective::EffectiveGit;
use super::git::git;

/// Merge `task`'s branch into base per `cfg.merge`, push, and return the merged
/// commit sha.
///
/// The repo, base branch, and merge strategy come from `eff` (the per-workflow
/// resolved settings) so a workflow targeting a different repo merges in the right
/// place with its own strategy; only the remote stays global (`cfg`).
///
/// # Errors
/// Returns an error if any git step fails; the caller turns it into a `Block`.
pub async fn land(task: &Task, eff: &EffectiveGit, cfg: &EngineConfig) -> anyhow::Result<String> {
    let repo = eff.repo.as_path();
    let branch = task
        .branch
        .clone()
        .ok_or_else(|| anyhow::anyhow!("task {} has no branch to merge", task.id))?;

    match eff.merge {
        MergeMode::Pr => {
            // Push the branch from the task's WORKTREE, not the main repo. The
            // branch is checked out in the worktree (and for Shared mode it only
            // exists because the worktree was added on it); pushing from the main
            // checkout fails with "src refspec does not match any" when the main
            // repo's view of the ref lags or the branch lives only in the worktree.
            // The worktree shares the repo's refs, so the push reaches the same
            // remote. A PR is opened out of band. Record the branch head.
            let push_dir = task.worktree.as_deref().map_or(repo, std::path::Path::new);
            push(push_dir, &cfg.remote, &branch).await?;
            head_sha(task, repo, &branch).await
        }
        MergeMode::FastForward | MergeMode::Merge => {
            // Move onto base, integrate the branch, push base, then push the
            // branch too so the remote keeps the task ref.
            checkout(repo, &eff.base_branch).await?;
            let args: Vec<&str> = match eff.merge {
                MergeMode::FastForward => vec!["merge", "--ff-only", &branch],
                // A merge commit keeps history when base has moved under us.
                _ => vec!["merge", "--no-edit", &branch],
            };
            let out = git(repo, &args).await?;
            if !out.ok {
                anyhow::bail!(
                    "git merge of {branch} into {} failed: {}",
                    eff.base_branch,
                    out.stderr
                );
            }
            push(repo, &cfg.remote, &eff.base_branch).await?;
            let sha = rev_parse(repo, "HEAD").await?;
            Ok(sha)
        }
    }
}

/// Commit any uncommitted work in `task`'s worktree before landing.
///
/// `land()` only ever pushes/merges the branch *head*; it does not commit. If the
/// agent finished its work but left it staged or unstaged (or never ran `git
/// commit` at all), that work would be silently dropped — the PR would show fewer
/// commits than tasks, or an empty one. So the engine commits for it: `git add -A`
/// then, if the tree now has staged changes, `git commit` with a generated message.
///
/// Returns:
/// - `Ok(Some(sha))` — there were changes; they are now one commit, whose sha is
///   returned (it becomes the branch head `land()` then records).
/// - `Ok(None)` — the worktree was clean (nothing staged or unstaged). The caller
///   treats this as a no-op task and blocks it rather than landing empty work.
///
/// Runs in the task's own worktree (`worktree`), not the main repo, so a `Shared`
/// run accumulates one commit per task on the shared branch in sequence.
///
/// # Errors
/// Returns an error if a git step fails; the caller turns it into a `Block`.
pub async fn commit_worktree(
    worktree: &std::path::Path,
    task: &Task,
) -> anyhow::Result<Option<String>> {
    let add = git(worktree, &["add", "-A"]).await?;
    if !add.ok {
        anyhow::bail!(
            "git add -A in {} failed: {}",
            worktree.display(),
            add.stderr
        );
    }
    // Never commit the engine's own bootstrap file: `provision` writes a scoped
    // `.claude/settings.json` into a fresh worktree so the headless agent doesn't
    // stall on approval prompts. It's infrastructure, not the task's work —
    // committing it would pollute every PR and make a no-op task (agent did
    // nothing) look like real work, defeating the empty-task check below. Unstage
    // it so only the agent's actual changes count. `--ignore-unmatch` keeps this a
    // no-op when the repo committed its own posture (no bootstrap was written).
    let _ = git(
        worktree,
        &["reset", "-q", "--", ".claude/settings.json"],
    )
    .await;
    // Nothing staged after `add -A` ⇒ the worktree had no changes at all. Distinct
    // from "agent already committed": in that case there's also nothing staged, but
    // the branch is ahead of base — the caller doesn't reach here for that because
    // it only auto-commits, and a clean tree with prior commits still lands them.
    // We report `None` purely on "no staged changes now"; an already-committed task
    // is `None` here yet still has its real commits, which `land()` records.
    let staged = git(worktree, &["diff", "--cached", "--quiet"]).await?;
    if staged.ok {
        // Exit 0 from `diff --cached --quiet` == no staged changes == nothing to
        // commit. Let the caller decide (no-op vs. already-committed) via the branch.
        return Ok(None);
    }
    let msg = format!("task({}): {}", task.id, task.title);
    let commit = git(worktree, &["commit", "-m", &msg]).await?;
    if !commit.ok {
        anyhow::bail!(
            "git commit in {} failed: {}",
            worktree.display(),
            commit.stderr
        );
    }
    let sha = rev_parse(worktree, "HEAD").await?;
    Ok(Some(sha))
}

/// Whether `task`'s branch is ahead of `base` — i.e. it carries at least one commit
/// to land. Used to distinguish a genuinely empty task (clean tree *and* no commits
/// ahead) from one whose agent already committed (clean tree but commits ahead).
///
/// # Errors
/// Returns an error only if git cannot be launched.
pub async fn branch_has_commits(
    worktree: &std::path::Path,
    base: &str,
    branch: &str,
) -> anyhow::Result<bool> {
    let range = format!("{base}..{branch}");
    let out = git(worktree, &["rev-list", "--count", &range]).await?;
    if !out.ok {
        // Base or branch unknown from here — be permissive (assume work exists)
        // rather than wrongly flag a real task as empty.
        return Ok(true);
    }
    Ok(out.stdout.trim() != "0")
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
///
/// If `remote` is not configured on the repo (e.g. a purely local target with no
/// `origin`), the push is **skipped** rather than failed: landing locally is a
/// valid end state. A configured remote whose push genuinely fails (auth, network,
/// rejected) is still surfaced as an error.
async fn push(repo: &std::path::Path, remote: &str, refname: &str) -> anyhow::Result<()> {
    // Probe the remote first; absence is not an error, just "nothing to push to".
    let has_remote = git(repo, &["remote", "get-url", remote])
        .await
        .map(|o| o.ok)
        .unwrap_or(false);
    if !has_remote {
        tracing::warn!(
            remote,
            refname,
            "no such remote configured; skipping push (landed locally)"
        );
        return Ok(());
    }
    let out = git(repo, &["push", remote, refname]).await?;
    if !out.ok {
        anyhow::bail!("git push {remote} {refname} failed: {}", out.stderr);
    }
    Ok(())
}

/// Resolve the branch head, preferring the task's worktree if it has one.
async fn head_sha(task: &Task, repo: &std::path::Path, branch: &str) -> anyhow::Result<String> {
    let repo = task.worktree.as_deref().map_or(repo, std::path::Path::new);
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
            agent_model: None,
            agent_effort: None,
            permission_flags: std::collections::HashMap::new(),
            auto_trust_agent_folder: true,
            stale_after_secs: 300,
            tick_secs: 2,
            issue_sync_every_n_ticks: 0,
        }
    }

    #[tokio::test]
    async fn fast_forward_merges_branch_into_base() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        // A task branch with one commit ahead of main.
        git(dir.path(), &["checkout", "-b", "lazy/auth"])
            .await
            .unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        git(dir.path(), &["add", "."]).await.unwrap();
        git(dir.path(), &["commit", "-m", "work"]).await.unwrap();
        let want = rev_parse(dir.path(), "HEAD").await.unwrap();

        let cfg = cfg_for(dir.path(), MergeMode::FastForward);
        let mut task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        task.branch = Some("lazy/auth".into());
        let eff = EffectiveGit {
            repo: dir.path().to_path_buf(),
            base_branch: "main".into(),
            branch_prefix: "lazy/".into(),
            worktree_mode: lazybones_store::WorktreeMode::New,
            tool: "claude".into(),
            model: None,
            effort: None,
            gate: vec![],
            merge: MergeMode::FastForward,
            auto_trust_agent_folder: true,
        };
        // No remote configured, so the push step fails — assert the merge itself
        // landed by checking base before the push error.
        let _ = land(&task, &eff, &cfg).await;
        let base = rev_parse(dir.path(), "main").await.unwrap();
        assert_eq!(base, want, "main should fast-forward to the branch head");
    }

    /// The merge strategy is resolved per-workflow on `eff`, not from the global
    /// `cfg`: a global `fast-forward` config must not stop a workflow whose
    /// effective mode is `merge` from integrating a *diverged* branch.
    #[tokio::test]
    async fn effective_merge_mode_overrides_global_for_diverged_branch() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        // Branch off, then move main forward too so the branch can't fast-forward.
        git(dir.path(), &["checkout", "-b", "lazy/auth"])
            .await
            .unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        git(dir.path(), &["add", "."]).await.unwrap();
        git(dir.path(), &["commit", "-m", "branch work"])
            .await
            .unwrap();
        git(dir.path(), &["checkout", "main"]).await.unwrap();
        std::fs::write(dir.path().join("b.txt"), "b").unwrap();
        git(dir.path(), &["add", "."]).await.unwrap();
        git(dir.path(), &["commit", "-m", "base moved"])
            .await
            .unwrap();

        // Global config says fast-forward (which would fail on divergence)...
        let cfg = cfg_for(dir.path(), MergeMode::FastForward);
        let mut task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        task.branch = Some("lazy/auth".into());
        // ...but the workflow's effective mode is `merge`, which must win.
        let eff = EffectiveGit {
            repo: dir.path().to_path_buf(),
            base_branch: "main".into(),
            branch_prefix: "lazy/".into(),
            worktree_mode: lazybones_store::WorktreeMode::New,
            tool: "claude".into(),
            model: None,
            effort: None,
            gate: vec![],
            merge: MergeMode::Merge,
            auto_trust_agent_folder: true,
        };
        // No remote, so push is skipped; the merge itself must succeed.
        land(&task, &eff, &cfg).await.unwrap();
        // Both files are present on main → a merge commit integrated the branch.
        assert!(dir.path().join("a.txt").exists());
        assert!(dir.path().join("b.txt").exists());
    }

    /// Build an `EffectiveGit` for `repo`/`merge` with the rest defaulted.
    fn eff_for(repo: &Path, merge: MergeMode) -> EffectiveGit {
        EffectiveGit {
            repo: repo.to_path_buf(),
            base_branch: "main".into(),
            branch_prefix: "lazy/".into(),
            worktree_mode: lazybones_store::WorktreeMode::New,
            tool: "claude".into(),
            model: None,
            effort: None,
            gate: vec![],
            merge,
            auto_trust_agent_folder: true,
        }
    }

    /// Init a bare repo to act as a pushable `origin` (no network needed).
    async fn init_bare(dir: &Path) {
        git(dir, &["init", "--bare", "-b", "main"]).await.unwrap();
    }

    /// The push path was previously only exercised on a remote-less local repo.
    /// With a real (local-bare) `origin`, landing must push base to the remote and
    /// the merged commit must actually arrive there.
    #[tokio::test]
    async fn land_pushes_to_a_configured_remote() {
        let remote = tempfile::tempdir().unwrap();
        init_bare(remote.path()).await;
        let work = tempfile::tempdir().unwrap();
        init_repo(work.path()).await;
        // Wire origin → the bare repo and seed it with the initial main.
        git(
            work.path(),
            &["remote", "add", "origin", &remote.path().to_string_lossy()],
        )
        .await
        .unwrap();
        git(work.path(), &["push", "origin", "main"]).await.unwrap();

        // A task branch one commit ahead.
        git(work.path(), &["checkout", "-b", "lazy/auth"])
            .await
            .unwrap();
        std::fs::write(work.path().join("a.txt"), "a").unwrap();
        git(work.path(), &["add", "."]).await.unwrap();
        git(work.path(), &["commit", "-m", "work"]).await.unwrap();
        let want = rev_parse(work.path(), "HEAD").await.unwrap();

        let cfg = cfg_for(work.path(), MergeMode::FastForward);
        let mut task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        task.branch = Some("lazy/auth".into());
        let eff = eff_for(work.path(), MergeMode::FastForward);

        // Land must succeed *including* the push (no skip).
        let sha = land(&task, &eff, &cfg).await.unwrap();
        assert_eq!(sha, want);
        // The remote's main now points at the landed commit — the push really ran.
        // `ls-remote` from the work repo reads origin's ref without entering the
        // bare repo (which git's `safe.bareRepository` guard would block).
        let out = git(work.path(), &["ls-remote", "origin", "refs/heads/main"])
            .await
            .unwrap();
        assert!(out.ok, "ls-remote origin failed: {}", out.stderr);
        let remote_sha = out.stdout.split_whitespace().next().unwrap_or_default();
        assert_eq!(
            remote_sha, want,
            "origin/main should have the landed commit"
        );
    }

    /// Auto-commit: an agent that finished green but left its work *uncommitted*
    /// (staged or unstaged) must have it committed by the engine before landing —
    /// the work must not be silently dropped. Returns the new commit's sha.
    #[tokio::test]
    async fn commit_worktree_commits_uncommitted_work() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        git(dir.path(), &["checkout", "-b", "lazy/auth"])
            .await
            .unwrap();
        let before = rev_parse(dir.path(), "HEAD").await.unwrap();
        // Agent edited a file but never committed (left it unstaged).
        std::fs::write(dir.path().join("work.txt"), "new work").unwrap();

        let mut task = Task::seed("auth", "r", "Add auth", "s", vec![], vec![], None);
        task.branch = Some("lazy/auth".into());
        let sha = commit_worktree(dir.path(), &task).await.unwrap();

        let sha = sha.expect("uncommitted work should produce a commit");
        let head = rev_parse(dir.path(), "HEAD").await.unwrap();
        assert_eq!(sha, head, "returned sha is the new branch head");
        assert_ne!(sha, before, "a real commit advanced the branch");
        // The generated message carries the task id + title.
        let msg = git(dir.path(), &["log", "-1", "--pretty=%s"])
            .await
            .unwrap();
        assert_eq!(msg.stdout, "task(auth): Add auth");
    }

    /// A truly empty task (clean tree, nothing ahead of base) reports `None` from
    /// `commit_worktree` and `false` from `branch_has_commits` — the caller flags it.
    #[tokio::test]
    async fn commit_worktree_reports_noop_for_clean_tree() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        git(dir.path(), &["checkout", "-b", "lazy/noop"])
            .await
            .unwrap();

        let mut task = Task::seed("noop", "r", "Nothing", "s", vec![], vec![], None);
        task.branch = Some("lazy/noop".into());
        let res = commit_worktree(dir.path(), &task).await.unwrap();
        assert!(res.is_none(), "a clean tree has nothing to commit");
        // And the branch carries no commits ahead of base → genuinely empty.
        let ahead = branch_has_commits(dir.path(), "main", "lazy/noop")
            .await
            .unwrap();
        assert!(!ahead, "no commits ahead of base → empty task");
    }

    /// A clean tree but the agent *already committed* is NOT a no-op: `commit_worktree`
    /// returns `None` (nothing to add) yet `branch_has_commits` is `true`, so the
    /// caller lands the existing commits rather than flagging an empty task.
    #[tokio::test]
    async fn already_committed_task_is_not_flagged_empty() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        git(dir.path(), &["checkout", "-b", "lazy/done"])
            .await
            .unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        git(dir.path(), &["add", "."]).await.unwrap();
        git(dir.path(), &["commit", "-m", "agent already did this"])
            .await
            .unwrap();

        let mut task = Task::seed("done", "r", "t", "s", vec![], vec![], None);
        task.branch = Some("lazy/done".into());
        let res = commit_worktree(dir.path(), &task).await.unwrap();
        assert!(res.is_none(), "nothing new to commit");
        let ahead = branch_has_commits(dir.path(), "main", "lazy/done")
            .await
            .unwrap();
        assert!(
            ahead,
            "the agent's own commit is ahead of base → land it, not flag"
        );
    }

    /// Shared mode: sequential tasks on ONE branch each contribute one commit. Two
    /// auto-commits in the same tree leave the branch two commits ahead of base.
    #[tokio::test]
    async fn shared_branch_accumulates_one_commit_per_task() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        // The single shared branch for the run.
        git(dir.path(), &["checkout", "-b", "lazy/run-1"])
            .await
            .unwrap();

        // Task 1 leaves uncommitted work; engine commits it.
        std::fs::write(dir.path().join("t1.txt"), "1").unwrap();
        let mut t1 = Task::seed("t1", "r", "First", "s", vec![], vec![], None);
        t1.branch = Some("lazy/run-1".into());
        commit_worktree(dir.path(), &t1)
            .await
            .unwrap()
            .expect("t1 commit");

        // Task 2 builds on the same tree/branch, also uncommitted.
        std::fs::write(dir.path().join("t2.txt"), "2").unwrap();
        let mut t2 = Task::seed("t2", "r", "Second", "s", vec![], vec![], None);
        t2.branch = Some("lazy/run-1".into());
        commit_worktree(dir.path(), &t2)
            .await
            .unwrap()
            .expect("t2 commit");

        // Two tasks → exactly two commits ahead of base on the shared branch.
        let out = git(dir.path(), &["rev-list", "--count", "main..lazy/run-1"])
            .await
            .unwrap();
        assert_eq!(
            out.stdout, "2",
            "one commit per task accumulated on the shared branch"
        );
    }

    /// A *configured* remote whose push genuinely fails must error — never be
    /// silently skipped (that is the missing-remote case, which is a valid skip).
    #[tokio::test]
    async fn land_errors_when_a_configured_push_fails() {
        let work = tempfile::tempdir().unwrap();
        init_repo(work.path()).await;
        // origin is configured but points at a path that is not a repo → push fails.
        git(
            work.path(),
            &["remote", "add", "origin", "/no/such/remote/repo"],
        )
        .await
        .unwrap();
        git(work.path(), &["checkout", "-b", "lazy/auth"])
            .await
            .unwrap();
        std::fs::write(work.path().join("a.txt"), "a").unwrap();
        git(work.path(), &["add", "."]).await.unwrap();
        git(work.path(), &["commit", "-m", "work"]).await.unwrap();

        let cfg = cfg_for(work.path(), MergeMode::FastForward);
        let mut task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        task.branch = Some("lazy/auth".into());
        let eff = eff_for(work.path(), MergeMode::FastForward);

        let err = land(&task, &eff, &cfg).await.unwrap_err();
        assert!(
            err.to_string().contains("git push"),
            "a configured-but-failing push must surface as an error, got: {err}"
        );
    }
}
