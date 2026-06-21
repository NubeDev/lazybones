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
//! tool          = task.tool                     ?? run.workspace.tool   ?? global agent_tool
//! model         = task.model                    ?? run.workspace.model  ?? global agent_model
//! effort        = task.effort                   ?? run.workspace.effort ?? global agent_effort
//! gate          = run.workspace.gate            ?? global gate
//! ```
//!
//! `merge` resolves `run.workspace.merge ?? global merge` — EXCEPT a `Shared`
//! worktree mode forces `merge = pr` (see `merge_for`): one shared branch only
//! makes sense as one PR, so it's pushed, never auto-merged into base.
//!
//! The gate has no task layer: it is a property of the *workspace's* stack (a Rust
//! workflow gates with `cargo`, a Node one with `npm`), so every task in a workflow
//! shares it. `Some([])` on the workspace is an explicit **no gate**; `None` inherits
//! the global default.
//!
//! The agent triple (`tool`/`model`/`effort`) follows the same most-specific-wins
//! rule so a workflow or template can set a default that every task inherits, and
//! any task can still override it. `tool` always resolves to a value (global has a
//! default); `model`/`effort` may stay `None` (the agent CLI then uses its own).
//!
//! A **standalone task** (`run = None`) reads only the global config and its own
//! `worktree_mode` — byte-identical to the pre-workflow behaviour. The repo lives
//! on the *workspace*, never the task, because two workflows can target the same
//! repo with different modes.

use std::path::PathBuf;

use lazybones_store::{MergeMode as StoreMergeMode, Run, Task, WorktreeMode};

use crate::config::{EngineConfig, MergeMode};

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
    /// The agent tool to launch (always set: global has a default).
    pub tool: String,
    /// The model forwarded to the agent CLI; `None` = the CLI's own default.
    pub model: Option<String>,
    /// The effort forwarded to the agent CLI; `None` = the CLI's own default.
    pub effort: Option<String>,
    /// The green-build gate commands to run in the worktree before landing. An
    /// empty `Vec` means **no gate** (land on agent DONE); resolved from the
    /// workflow's workspace, else the global `EngineConfig.gate`.
    pub gate: Vec<String>,
    /// How a green branch lands back on base; resolved from the workflow's
    /// workspace, else the global `EngineConfig.merge`. Lets one repo pin strict
    /// `fast-forward` while a concurrent workflow uses `merge`.
    pub merge: MergeMode,
    /// Whether to pre-seed Claude Code's folder-trust flag for this task's
    /// worktree before spawning. Resolved task ?? global; lets one task opt out of
    /// the (default-on) auto-trust without disabling it for the whole daemon.
    pub auto_trust_agent_folder: bool,
}

/// Map the storable workspace merge mode onto the engine's config enum (the two
/// are deliberately separate: the store may not depend on the engine).
fn map_merge(m: StoreMergeMode) -> MergeMode {
    match m {
        StoreMergeMode::FastForward => MergeMode::FastForward,
        StoreMergeMode::Merge => MergeMode::Merge,
        StoreMergeMode::Pr => MergeMode::Pr,
    }
}

/// Reconcile the merge strategy with the worktree mode.
///
/// `Shared` mode means "all tasks on one branch → one PR". Auto-merging that
/// branch into base as each task finishes (`FastForward`/`Merge`) would advance
/// base under the still-live shared tree and, by the time the run ends, leave the
/// branch with zero commits ahead of base — nothing to open a PR from. So a
/// `Shared` workflow always lands as `Pr`: push the shared branch, never
/// auto-merge, and leave the single PR for a human. Other modes keep whatever
/// strategy was resolved.
fn merge_for(worktree_mode: WorktreeMode, resolved: MergeMode) -> MergeMode {
    match worktree_mode {
        WorktreeMode::Shared => MergeMode::Pr,
        _ => resolved,
    }
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
            // No workspace layer: task ?? global.
            tool: task.tool.clone().unwrap_or_else(|| cfg.agent_tool.clone()),
            model: task.model.clone().or_else(|| cfg.agent_model.clone()),
            effort: task.effort.clone().or_else(|| cfg.agent_effort.clone()),
            // Standalone: no workspace gate; the global default is the contract.
            gate: cfg.gate.clone(),
            // Standalone: no workspace merge; the global default is the contract.
            merge: cfg.merge,
            // task ?? global. No workspace layer — it's a per-task safety knob.
            auto_trust_agent_folder: task
                .auto_trust_agent_folder
                .unwrap_or(cfg.auto_trust_agent_folder),
        },
        Some(run) => {
            let ws = &run.workspace;
            // Task override wins; else the workspace default.
            let worktree_mode = task.worktree_mode_override.unwrap_or(ws.worktree_mode);
            EffectiveGit {
                repo: PathBuf::from(&ws.repo),
                base_branch: ws.base_branch.clone().unwrap_or_else(|| cfg.base_branch.clone()),
                branch_prefix: ws
                    .branch_prefix
                    .clone()
                    .unwrap_or_else(|| cfg.branch_prefix.clone()),
                worktree_mode,
                // task ?? workspace ?? global, per field.
                tool: task
                    .tool
                    .clone()
                    .or_else(|| ws.tool.clone())
                    .unwrap_or_else(|| cfg.agent_tool.clone()),
                model: task
                    .model
                    .clone()
                    .or_else(|| ws.model.clone())
                    .or_else(|| cfg.agent_model.clone()),
                effort: task
                    .effort
                    .clone()
                    .or_else(|| ws.effort.clone())
                    .or_else(|| cfg.agent_effort.clone()),
                // workspace ?? global. `Some([])` is an explicit no-gate, so we
                // honour the present-but-empty list rather than falling back.
                gate: ws.gate.clone().unwrap_or_else(|| cfg.gate.clone()),
                // workspace ?? global merge strategy — but `Shared` forces `Pr`
                // (see `merge_for`): auto-merging task branches into base mid-run
                // would defeat the whole "one shared branch → one PR" contract.
                merge: merge_for(worktree_mode, ws.merge.map_or(cfg.merge, map_merge)),
                // task ?? global. No workspace layer — it's a per-task safety knob.
                auto_trust_agent_folder: task
                    .auto_trust_agent_folder
                    .unwrap_or(cfg.auto_trust_agent_folder),
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
            agent_model: None,
            agent_effort: None,
            permission_flags: std::collections::HashMap::new(),
            auto_trust_agent_folder: true,
            stale_after_secs: 300,
            tick_secs: 2,
            issue_sync_every_n_ticks: 0,
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
            tool: None,
            model: None,
            effort: None,
            gate: None,
            merge: None,
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
            tool: None,
            model: None,
            effort: None,
            gate: None,
            merge: None,
        });
        let eff = resolve(&task, Some(&run), &cfg());
        assert_eq!(eff.base_branch, "dev");
        assert_eq!(eff.branch_prefix, "wf/");
        assert_eq!(eff.worktree_mode, WorktreeMode::Reuse);
    }

    #[test]
    fn agent_triple_resolves_task_then_workspace_then_global() {
        // Global has tool=claude, model=None, effort=None.
        let mut cfg = cfg();
        cfg.agent_model = Some("global-model".into());

        // Workspace sets a tool + effort default; leaves model to inherit global.
        let run = run_with(Workspace {
            repo: "/repo/abc".into(),
            base_branch: None,
            branch_prefix: None,
            worktree_mode: WorktreeMode::New,
            tool: Some("codex".into()),
            model: None,
            effort: Some("medium".into()),
            gate: None,
            merge: None,
        });

        // A task with no agent fields inherits workspace tool/effort + global model.
        let bare = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        let eff = resolve(&bare, Some(&run), &cfg);
        assert_eq!(eff.tool, "codex");
        assert_eq!(eff.model.as_deref(), Some("global-model"));
        assert_eq!(eff.effort.as_deref(), Some("medium"));

        // A task that sets its own fields wins over both layers.
        let mut over = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        over.tool = Some("gemini".into());
        over.model = Some("task-model".into());
        over.effort = Some("high".into());
        let eff = resolve(&over, Some(&run), &cfg);
        assert_eq!(eff.tool, "gemini");
        assert_eq!(eff.model.as_deref(), Some("task-model"));
        assert_eq!(eff.effort.as_deref(), Some("high"));
    }

    #[test]
    fn standalone_agent_triple_is_task_then_global() {
        let mut cfg = cfg();
        cfg.agent_effort = Some("global-effort".into());
        let mut task = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        task.tool = Some("codex".into());
        let eff = resolve(&task, None, &cfg);
        assert_eq!(eff.tool, "codex"); // task wins
        assert_eq!(eff.model, None); // nothing set anywhere
        assert_eq!(eff.effort.as_deref(), Some("global-effort")); // global fills in
    }

    #[test]
    fn auto_trust_resolves_task_then_global() {
        // Global on by default.
        let cfg = cfg();
        let bare = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        assert!(resolve(&bare, None, &cfg).auto_trust_agent_folder);

        // A task can opt out without touching the global.
        let mut off = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        off.auto_trust_agent_folder = Some(false);
        assert!(!resolve(&off, None, &cfg).auto_trust_agent_folder);

        // The same override holds inside a workflow run (no workspace layer).
        let run = run_with(Workspace {
            repo: "/repo/abc".into(),
            base_branch: None,
            branch_prefix: None,
            worktree_mode: WorktreeMode::New,
            tool: None,
            model: None,
            effort: None,
            gate: None,
            merge: None,
        });
        assert!(!resolve(&off, Some(&run), &cfg).auto_trust_agent_folder);
        assert!(resolve(&bare, Some(&run), &cfg).auto_trust_agent_folder);
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
            tool: None,
            model: None,
            effort: None,
            gate: None,
            merge: None,
        });
        let eff = resolve(&task, Some(&run), &cfg());
        assert_eq!(eff.worktree_mode, WorktreeMode::Branch);
    }

    /// A workspace with `gate` set; every other field left to inherit.
    fn ws_with_gate(gate: Option<Vec<String>>) -> Workspace {
        Workspace {
            repo: "/repo/abc".into(),
            base_branch: None,
            branch_prefix: None,
            worktree_mode: WorktreeMode::New,
            tool: None,
            model: None,
            effort: None,
            gate,
            merge: None,
        }
    }

    #[test]
    fn gate_resolves_workspace_then_empty_then_global() {
        let mut cfg = cfg();
        cfg.gate = vec!["cargo test --workspace".into()];
        let task = Task::seed("t", "r", "T", "s", vec![], vec![], None);

        // Workspace gate wins over the global default.
        let run = run_with(ws_with_gate(Some(vec!["npm test".into()])));
        assert_eq!(resolve(&task, Some(&run), &cfg).gate, vec!["npm test".to_owned()]);

        // Explicit empty list is honoured as "no gate", not a fallback to global.
        let run = run_with(ws_with_gate(Some(vec![])));
        assert!(resolve(&task, Some(&run), &cfg).gate.is_empty());

        // Absent (None) inherits the global gate.
        let run = run_with(ws_with_gate(None));
        assert_eq!(
            resolve(&task, Some(&run), &cfg).gate,
            vec!["cargo test --workspace".to_owned()]
        );

        // A standalone task always uses the global gate.
        assert_eq!(
            resolve(&task, None, &cfg).gate,
            vec!["cargo test --workspace".to_owned()]
        );
    }

    /// A workspace with `merge` set; every other field left to inherit.
    fn ws_with_merge(merge: Option<StoreMergeMode>) -> Workspace {
        Workspace {
            repo: "/repo/abc".into(),
            base_branch: None,
            branch_prefix: None,
            worktree_mode: WorktreeMode::New,
            tool: None,
            model: None,
            effort: None,
            gate: None,
            merge,
        }
    }

    #[test]
    fn merge_resolves_workspace_then_global() {
        // Global default is fast-forward; the workspace can pin a different one.
        let mut cfg = cfg();
        cfg.merge = MergeMode::FastForward;
        let task = Task::seed("t", "r", "T", "s", vec![], vec![], None);

        // Workspace merge wins over the global default.
        let run = run_with(ws_with_merge(Some(StoreMergeMode::Merge)));
        assert_eq!(resolve(&task, Some(&run), &cfg).merge, MergeMode::Merge);

        let run = run_with(ws_with_merge(Some(StoreMergeMode::Pr)));
        assert_eq!(resolve(&task, Some(&run), &cfg).merge, MergeMode::Pr);

        // Absent (None) inherits the global merge — a repo keeps strict linear
        // history while a sibling workflow opts into merge commits.
        let run = run_with(ws_with_merge(None));
        assert_eq!(resolve(&task, Some(&run), &cfg).merge, MergeMode::FastForward);

        // A standalone task always uses the global merge.
        assert_eq!(resolve(&task, None, &cfg).merge, MergeMode::FastForward);
    }

    #[test]
    fn shared_worktree_forces_merge_pr() {
        // A `Shared` workflow must land as `Pr` no matter what merge was set —
        // auto-merging the shared branch mid-run would leave nothing to PR.
        let mut cfg = cfg();
        cfg.merge = MergeMode::Merge; // global says auto-merge
        let task = Task::seed("t", "r", "T", "s", vec![], vec![], None);

        // Workspace is Shared with no explicit merge → coerced to Pr (not the
        // inherited global `Merge`).
        let mut ws = ws_with_merge(None);
        ws.worktree_mode = WorktreeMode::Shared;
        assert_eq!(
            resolve(&task, Some(&run_with(ws)), &cfg).merge,
            MergeMode::Pr,
        );

        // Even an EXPLICIT fast-forward on a Shared workflow is overridden — the
        // two are incoherent together, and Pr is the only mode that preserves the
        // branch for review.
        let mut ws = ws_with_merge(Some(StoreMergeMode::FastForward));
        ws.worktree_mode = WorktreeMode::Shared;
        assert_eq!(
            resolve(&task, Some(&run_with(ws)), &cfg).merge,
            MergeMode::Pr,
        );

        // A task-level override to Shared also forces Pr, even when the workspace
        // is an auto-merging non-Shared mode.
        let mut shared_task = Task::seed("t", "r", "T", "s", vec![], vec![], None);
        shared_task.worktree_mode_override = Some(WorktreeMode::Shared);
        let run = run_with(ws_with_merge(Some(StoreMergeMode::Merge)));
        assert_eq!(
            resolve(&shared_task, Some(&run), &cfg).merge,
            MergeMode::Pr,
        );

        // Sanity: a non-Shared workspace still honours its merge mode.
        let mut ws = ws_with_merge(Some(StoreMergeMode::Merge));
        ws.worktree_mode = WorktreeMode::New;
        assert_eq!(
            resolve(&task, Some(&run_with(ws)), &cfg).merge,
            MergeMode::Merge,
        );
    }
}
