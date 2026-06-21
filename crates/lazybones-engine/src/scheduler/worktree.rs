//! Provision (and tear down) a task's working tree per its [`WorktreeMode`].
//!
//! The contract is from [starting-tasks.md]; all git runs with `-C target_repo`.
//!
//! | mode   | action                                            | branch              | path              |
//! | ------ | ------------------------------------------------- | ------------------- | ----------------- |
//! | `New`  | `git worktree add <root>/<id> -b <prefix><id> <base>` | `<prefix><id>`  | `<repo>/<root>/<id>` |
//! | `Reuse`| use `task.worktree`; **block** if missing/not a dir | `task.branch`     | `task.worktree`   |
//! | `Branch`| `git checkout -B <branch> <base>` in the main checkout | `task.branch` or `<prefix><id>` | `<repo>` |
//!
//! `worktrees == false` forces `Branch` semantics (the serial fallback).

use std::path::{Path, PathBuf};

use lazybones_store::{Task, WorktreeMode};

use crate::config::EngineConfig;

use super::effective::EffectiveGit;
use super::git::git;

/// The scoped Claude Code allow-list written into a fresh worktree so a headless
/// agent runs without stalling on per-tool approval prompts (docs/issues/01).
///
/// This is **repo-scoped** — it lives in the worktree's `.claude/settings.json`,
/// not in `~/.claude.json`, and never enables the global bypass-mode flag. It is
/// only written when the target repo has no `.claude/settings.json` of its own, so
/// a repo that commits its own posture keeps full control.
///
/// The list must cover every tool a headless agent reaches for, or it parks at an
/// approval prompt nobody can answer and the scheduler reaps it as "screen settled
/// before readiness" — exactly the `launch_blocked` loop a review task hits when it
/// invokes `Skill`/`Task`/`TodoWrite`/`WebSearch` (a code-review agent runs the
/// review skill, spawns sub-agents, and tracks todos). So we allow the full set a
/// task agent uses, not just the file/shell primitives.
const CLAUDE_SETTINGS_BOOTSTRAP: &str = r#"{
  "permissions": {
    "allow": ["Bash", "Edit", "Write", "Read", "Glob", "Grep", "WebFetch", "WebSearch", "Skill", "Task", "TodoWrite", "NotebookEdit"]
  }
}
"#;

/// Where a claimed task will run and on which branch.
#[derive(Debug, Clone)]
pub struct Provisioned {
    /// Absolute path the agent edits in.
    pub worktree: String,
    /// The branch the agent commits to.
    pub branch: String,
}

/// Provision `task`'s working tree according to its *effective* git settings.
///
/// `eff` is the per-field-resolved repo/base/prefix/mode (task ?? workspace ??
/// global — see [`super::effective::resolve`]); `cfg` supplies only the
/// non-per-workflow knobs (`worktrees` toggle, `worktree_root`). `reuse_path` is
/// the worktree resolved from the task's `reuse_from` link, when set — the
/// scheduler reads the source task's stored worktree before calling this.
///
/// Done *before* the claim so a provisioning failure blocks cleanly with no
/// half-claimed task. Returns the worktree path and branch to record on claim.
///
/// # Errors
/// Returns an error if git fails or a `Reuse` path is missing — the caller turns
/// that into a `Block`.
pub async fn provision(
    task: &Task,
    eff: &EffectiveGit,
    cfg: &EngineConfig,
    reuse_path: Option<&str>,
) -> anyhow::Result<Provisioned> {
    let repo = &eff.repo;
    // `worktrees: false` collapses every mode to Branch (one checkout, serial).
    let mode = if cfg.worktrees {
        eff.worktree_mode
    } else {
        WorktreeMode::Branch
    };

    match mode {
        WorktreeMode::New => {
            let branch = format!("{}{}", eff.branch_prefix, task.id);
            let path: PathBuf = repo.join(&cfg.worktree_root).join(&task.id);
            let path_str = path.to_string_lossy().into_owned();
            // Idempotent across reclaims: if the worktree already exists, reuse it.
            if !path.is_dir() {
                // The branch may already exist (a prior run, or a restart that
                // removed the tree but not the branch). `worktree add -b` fails
                // hard on a pre-existing branch, so check first and add *onto*
                // the existing branch (no `-b`) when it's there — otherwise
                // create it fresh from base. Keeps re-Start from breaking.
                let exists = branch_exists(repo, &branch).await?;
                let add: Vec<&str> = if exists {
                    vec!["worktree", "add", &path_str, &branch]
                } else {
                    vec!["worktree", "add", &path_str, "-b", &branch, &eff.base_branch]
                };
                let out = git(repo, &add).await?;
                if !out.ok {
                    anyhow::bail!("git worktree add for {} failed: {}", task.id, out.stderr);
                }
            }
            bootstrap_permissions(repo, &path, &eff.tool);
            Ok(Provisioned {
                worktree: path_str,
                branch,
            })
        }
        WorktreeMode::Reuse => {
            // Prefer the `reuse_from` source task's tree; else the task's own.
            let path = reuse_path
                .map(ToOwned::to_owned)
                .or_else(|| task.worktree.clone())
                .ok_or_else(|| anyhow::anyhow!("reuse mode but task {} has no worktree", task.id))?;
            let reused = PathBuf::from(&path);
            if !reused.is_dir() {
                anyhow::bail!("reuse worktree {path} for {} is missing or not a dir", task.id);
            }
            let branch = task
                .branch
                .clone()
                .unwrap_or_else(|| format!("{}{}", eff.branch_prefix, task.id));
            // Establish the task's branch from the reused tree's current HEAD so
            // the agent commits onto a real ref and the later merge has a branch
            // to land (the reused tree continues from where its owner left off).
            let out = git(&reused, &["checkout", "-B", &branch]).await?;
            if !out.ok {
                anyhow::bail!("git checkout -B {branch} in reused tree for {} failed: {}", task.id, out.stderr);
            }
            bootstrap_permissions(repo, &reused, &eff.tool);
            Ok(Provisioned {
                worktree: path,
                branch,
            })
        }
        WorktreeMode::Branch => {
            let branch = task
                .branch
                .clone()
                .unwrap_or_else(|| format!("{}{}", eff.branch_prefix, task.id));
            let out = git(repo, &["checkout", "-B", &branch, &eff.base_branch]).await?;
            if !out.ok {
                anyhow::bail!("git checkout -B {branch} for {} failed: {}", task.id, out.stderr);
            }
            // Branch mode runs in the main checkout, so the worktree *is* the repo:
            // the "repo already has settings" guard makes this a no-op when the repo
            // commits its own posture, and a one-time create otherwise.
            bootstrap_permissions(repo, repo, &eff.tool);
            Ok(Provisioned {
                worktree: repo.to_string_lossy().into_owned(),
                branch,
            })
        }
    }
}

/// Write a scoped Claude Code allow-list into a fresh worktree so a headless
/// agent runs without stalling on per-tool approval prompts (docs/issues/01).
///
/// Only for the `claude` tool — other CLIs (codex, gemini, opencode) have their
/// own gate model and are bootstrapped through `permission_flags`, not this file.
/// Skipped entirely when the **target repo** already commits a
/// `.claude/settings.json`, so a repo that owns its posture is never overwritten.
/// Best-effort: a write failure is logged, not fatal — the agent can still run if
/// `permission_flags` or a pre-trusted environment covers the gate.
///
/// Repo-scoped by construction: it touches only `<worktree>/.claude/settings.json`
/// and never the global `~/.claude.json` bypass-mode flag.
fn bootstrap_permissions(repo: &Path, worktree: &Path, tool: &str) {
    if tool != "claude" {
        return;
    }
    // Respect a repo that commits its own posture — don't clobber it.
    if repo.join(".claude/settings.json").exists() {
        return;
    }
    let dir = worktree.join(".claude");
    let settings = dir.join("settings.json");
    // Idempotent across reclaims: if we already wrote one, leave it.
    if settings.exists() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&settings, CLAUDE_SETTINGS_BOOTSTRAP))
    {
        tracing::warn!(worktree = %worktree.display(), "failed to bootstrap .claude/settings.json: {e}");
    }
}

/// Whether a local branch `name` exists in `repo`.
///
/// Used by `New`-mode provisioning to decide between `worktree add -b` (create)
/// and `worktree add` onto an existing branch, so a leftover branch from a prior
/// run or a restart doesn't make the add fail.
///
/// # Errors
/// Returns an error only if git cannot be launched.
async fn branch_exists(repo: &Path, name: &str) -> anyhow::Result<bool> {
    let refname = format!("refs/heads/{name}");
    let out = git(repo, &["show-ref", "--verify", "--quiet", &refname]).await?;
    Ok(out.ok)
}

/// Tear down a task's worktree after a green merge (no-op for `Branch`, which
/// runs in the main checkout).
///
/// # Errors
/// Returns an error only if git cannot be launched; a non-zero removal is logged
/// by the caller, not fatal.
pub async fn teardown(task: &Task, eff: &EffectiveGit, cfg: &EngineConfig) -> anyhow::Result<()> {
    let mode = if cfg.worktrees {
        eff.worktree_mode
    } else {
        WorktreeMode::Branch
    };
    if matches!(mode, WorktreeMode::Branch) {
        return Ok(());
    }
    if let Some(path) = &task.worktree {
        let out = git(&eff.repo, &["worktree", "remove", "--force", path]).await?;
        if !out.ok {
            tracing::warn!(task = %task.id, "worktree remove failed: {}", out.stderr);
        }
    }
    Ok(())
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

    fn cfg_for(repo: &Path) -> EngineConfig {
        EngineConfig {
            target_repo: repo.to_path_buf(),
            base_branch: "main".into(),
            remote: "origin".into(),
            gate: vec![],
            concurrency: 3,
            worktrees: true,
            worktree_root: ".lazy/wt".into(),
            branch_prefix: "lazy/".into(),
            merge: crate::config::MergeMode::FastForward,
            agent_tool: "claude".into(),
            agent_model: None,
            agent_effort: None,
            permission_flags: std::collections::HashMap::new(),
            stale_after_secs: 300,
            tick_secs: 2,
        }
    }

    /// The standalone effective settings for a task (global repo, task's mode).
    fn eff_for(repo: &Path, mode: WorktreeMode) -> EffectiveGit {
        EffectiveGit {
            repo: repo.to_path_buf(),
            base_branch: "main".into(),
            branch_prefix: "lazy/".into(),
            worktree_mode: mode,
            tool: "claude".into(),
            model: None,
            effort: None,
            gate: vec![],
            merge: crate::config::MergeMode::FastForward,
        }
    }

    #[tokio::test]
    async fn new_mode_creates_worktree_and_branch() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        let cfg = cfg_for(dir.path());
        let task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        let eff = eff_for(dir.path(), WorktreeMode::New);
        let p = provision(&task, &eff, &cfg, None).await.unwrap();
        assert_eq!(p.branch, "lazy/auth");
        assert!(Path::new(&p.worktree).is_dir());
    }

    #[tokio::test]
    async fn new_mode_reprovisions_when_branch_already_exists() {
        // Regression: a restart (or a torn-down tree) can leave the branch
        // behind. The old `worktree add -b` failed with "branch already exists";
        // New mode now adds onto the existing branch instead.
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        let cfg = cfg_for(dir.path());
        let task = Task::seed("backend", "r", "t", "s", vec![], vec![], None);
        let eff = eff_for(dir.path(), WorktreeMode::New);

        // First provision creates worktree + branch `lazy/backend`.
        let p1 = provision(&task, &eff, &cfg, None).await.unwrap();
        assert_eq!(p1.branch, "lazy/backend");

        // Remove the tree but leave the branch (what a restart-with-teardown
        // used to do before we also deleted the branch).
        git(dir.path(), &["worktree", "remove", "--force", &p1.worktree])
            .await
            .unwrap();
        assert!(branch_exists(dir.path(), "lazy/backend").await.unwrap());

        // Re-provision must succeed onto the existing branch, not bail.
        let p2 = provision(&task, &eff, &cfg, None).await.unwrap();
        assert_eq!(p2.branch, "lazy/backend");
        assert!(Path::new(&p2.worktree).is_dir());
    }

    #[tokio::test]
    async fn reuse_missing_path_errors() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        let cfg = cfg_for(dir.path());
        let mut task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        task.worktree = Some("/no/such/dir".into());
        let eff = eff_for(dir.path(), WorktreeMode::Reuse);
        assert!(provision(&task, &eff, &cfg, None).await.is_err());
    }

    #[tokio::test]
    async fn reuse_from_path_is_preferred() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        let cfg = cfg_for(dir.path());
        let task = Task::seed("ui", "r", "t", "s", vec![], vec![], None);
        let eff = eff_for(dir.path(), WorktreeMode::Reuse);
        // The resolved `reuse_from` path (a real dir) wins over task.worktree.
        let p = provision(&task, &eff, &cfg, Some(&dir.path().to_string_lossy()))
            .await
            .unwrap();
        assert_eq!(p.worktree, dir.path().to_string_lossy());
    }

    #[tokio::test]
    async fn new_mode_bootstraps_claude_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        let cfg = cfg_for(dir.path());
        let task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        let eff = eff_for(dir.path(), WorktreeMode::New);
        let p = provision(&task, &eff, &cfg, None).await.unwrap();
        let settings = Path::new(&p.worktree).join(".claude/settings.json");
        assert!(settings.is_file(), "worktree should get a scoped allow-list");
        let body = std::fs::read_to_string(&settings).unwrap();
        assert!(body.contains("\"allow\""));
        assert!(body.contains("Bash"));
        // Repo-scoped only: nothing global is touched.
        assert!(!dir.path().join(".claude/settings.json").exists());
    }

    #[tokio::test]
    async fn repo_with_settings_is_not_clobbered() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        // The target repo commits its own posture.
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        let repo_settings = dir.path().join(".claude/settings.json");
        std::fs::write(&repo_settings, "{\"mine\":true}").unwrap();

        let cfg = cfg_for(dir.path());
        let task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        let eff = eff_for(dir.path(), WorktreeMode::New);
        let p = provision(&task, &eff, &cfg, None).await.unwrap();

        // The repo's own file is untouched, and no bootstrap is written into the wt.
        assert_eq!(std::fs::read_to_string(&repo_settings).unwrap(), "{\"mine\":true}");
        assert!(!Path::new(&p.worktree).join(".claude/settings.json").exists());
    }

    #[tokio::test]
    async fn non_claude_tool_is_not_bootstrapped() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        let cfg = cfg_for(dir.path());
        let task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        let mut eff = eff_for(dir.path(), WorktreeMode::New);
        eff.tool = "codex".into();
        let p = provision(&task, &eff, &cfg, None).await.unwrap();
        assert!(!Path::new(&p.worktree).join(".claude/settings.json").exists());
    }

    #[tokio::test]
    async fn branch_mode_runs_in_main_checkout() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        let cfg = cfg_for(dir.path());
        let task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        let eff = eff_for(dir.path(), WorktreeMode::Branch);
        let p = provision(&task, &eff, &cfg, None).await.unwrap();
        assert_eq!(p.worktree, dir.path().to_string_lossy());
        assert_eq!(p.branch, "lazy/auth");
    }
}
