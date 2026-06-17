//! Resolve a task's *effective* git settings at claim time.
//!
//! The one rule that makes concurrent workflows work (docs/workflows-scope.md):
//! a task's git config is resolved per field, **most-specific wins**, from the
//! task's parent [`Run`] workspace down to the global [`EngineConfig`]:
//!
//! ```text
//! repo          = run.workspace.repo            ?? global target_repo
//! base_branch   = run.workspace.base_branch     ?? global base_branch
//! branch_prefix = run.workspace.branch_prefix   ?? global branch_prefix
//! worktree_mode = task.worktree_mode_override   ?? run.workspace.worktree_mode ?? global default
//! ```
//!
//! A **standalone task** (`run = None`) reads only the global config and its own
//! `worktree_mode` — byte-identical to the pre-workflow behaviour. The repo lives
//! on the *workspace*, never the task, because two workflows can target the same
//! repo with different modes.

use std::path::PathBuf;

use lazybones_store::{Run, Task, WorktreeMode};

use crate::config::EngineConfig;

/// A task's resolved git settings — what `provision` actually uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveGit {
    /// The repo all `git` runs target.
    pub repo: PathBuf,
    /// The branch tasks fork from / merge into.
    pub base_branch: String,
    /// The branch-name prefix.
    pub branch_prefix: String,
    /// The worktree provisioning mode.
    pub worktree_mode: WorktreeMode,
}

/// Resolve `task`'s effective git settings given its (optional) parent `run` and
/// the global `cfg`.
///
/// For a standalone task (`run` is `None`), this returns the global repo/branch/
/// prefix and the task's own `worktree_mode` — exactly today's behaviour.
#[must_use]
pub fn resolve(task: &Task, run: Option<&Run>, cfg: &EngineConfig) -> EffectiveGit {
    match run {
        None => EffectiveGit {
            repo: cfg.target_repo.clone(),
            base_branch: cfg.base_branch.clone(),
            branch_prefix: cfg.branch_prefix.clone(),
            // Standalone: the task's own mode is the contract.
            worktree_mode: task.worktree_mode,
        },
        Some(run) => {
            let ws = &run.workspace;
            EffectiveGit {
                repo: PathBuf::from(&ws.repo),
                base_branch: ws.base_branch.clone().unwrap_or_else(|| cfg.base_branch.clone()),
                branch_prefix: ws
                    .branch_prefix
                    .clone()
                    .unwrap_or_else(|| cfg.branch_prefix.clone()),
                // Task override wins; else the workspace default.
                worktree_mode: task.worktree_mode_override.unwrap_or(ws.worktree_mode),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lazybones_store::Workspace;

    use super::*;

    fn cfg() -> EngineConfig {
        EngineConfig {
            target_repo: PathBuf::from("/global/repo"),
            base_branch: "main".into(),
            remote: "origin".into(),
            gate: vec![],
            concurrency: 3,
            worktrees: true,
            worktree_root: ".lazy/wt".into(),
            branch_prefix: "lazy/".into(),
            merge: crate::config::MergeMode::FastForward,
            agent_tool: "claude".into(),
            stale_after_secs: 300,
            tick_secs: 2,
        }
    }

    fn run_with(ws: Workspace) -> Run {
        Run::new("wf-1", "WF", ws, "2026-01-01T00:00:00Z")
    }

    #[test]
    fn standalone_uses_global_and_own_mode() {
        let mut task = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        task.worktree_mode = WorktreeMode::Branch;
        let eff = resolve(&task, None, &cfg());
        assert_eq!(eff.repo, PathBuf::from("/global/repo"));
        assert_eq!(eff.base_branch, "main");
        assert_eq!(eff.branch_prefix, "lazy/");
        assert_eq!(eff.worktree_mode, WorktreeMode::Branch);
    }

    #[test]
    fn workspace_repo_overrides_global() {
        let task = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        let run = run_with(Workspace {
            repo: "/repo/abc".into(),
            base_branch: None,
            branch_prefix: None,
            worktree_mode: WorktreeMode::New,
        });
        let eff = resolve(&task, Some(&run), &cfg());
        assert_eq!(eff.repo, PathBuf::from("/repo/abc"));
        // Unset workspace fields fall back to global.
        assert_eq!(eff.base_branch, "main");
        assert_eq!(eff.branch_prefix, "lazy/");
        assert_eq!(eff.worktree_mode, WorktreeMode::New);
    }

    #[test]
    fn workspace_branch_fields_override_global() {
        let task = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        let run = run_with(Workspace {
            repo: "/repo/abc".into(),
            base_branch: Some("dev".into()),
            branch_prefix: Some("wf/".into()),
            worktree_mode: WorktreeMode::Reuse,
        });
        let eff = resolve(&task, Some(&run), &cfg());
        assert_eq!(eff.base_branch, "dev");
        assert_eq!(eff.branch_prefix, "wf/");
        assert_eq!(eff.worktree_mode, WorktreeMode::Reuse);
    }

    #[test]
    fn task_override_beats_workspace_mode() {
        let mut task = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        task.worktree_mode_override = Some(WorktreeMode::Branch);
        let run = run_with(Workspace {
            repo: "/repo/abc".into(),
            base_branch: None,
            branch_prefix: None,
            worktree_mode: WorktreeMode::New,
        });
        let eff = resolve(&task, Some(&run), &cfg());
        assert_eq!(eff.worktree_mode, WorktreeMode::Branch);
    }
}
