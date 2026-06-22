//! Engine config: the scheduler keys from `lazybones.yaml` + `LAZYBONES_*` env.
//!
//! `configure.rs` in the CLI loads only the *boot* keys (bind, DB location). The
//! scheduler is the daemon now, so it must load the rest: the gate, concurrency,
//! worktree layout, and merge strategy. Same precedence as the boot config —
//! env wins over the file wins over the baked-in defaults.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// How a green task branch lands back on the base branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergeMode {
    /// Fast-forward `base` to the task branch; refuse if base moved (the default).
    #[default]
    FastForward,
    /// Create a merge commit of the task branch into `base`.
    Merge,
    /// Push the branch only; open a PR out of band (left to a human/reviewer).
    Pr,
}

impl MergeMode {
    /// Parse the wire form (`fast-forward` | `merge` | `pr`); unknown → default.
    fn parse(s: &str) -> Self {
        match s.trim() {
            "merge" => Self::Merge,
            "pr" => Self::Pr,
            // Default covers "fast-forward" and any typo — fail safe, not loud.
            _ => Self::FastForward,
        }
    }
}

/// The scheduler's configuration, parsed once at boot and shared by reference.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// The repo the agents build in; all `git` runs with `-C` here.
    pub target_repo: PathBuf,
    /// The branch tasks fork from and merge into.
    pub base_branch: String,
    /// The remote each task branch is pushed to.
    pub remote: String,
    /// Gate commands, run in order in the worktree; any non-zero exit blocks.
    pub gate: Vec<String>,
    /// Max agents running at once across the whole run.
    pub concurrency: usize,
    /// `false` forces serial execution on `base_branch` (degraded fallback).
    pub worktrees: bool,
    /// Where `git worktree add` lands, relative to `target_repo`.
    pub worktree_root: String,
    /// Branch-name prefix: task `auth` → `<prefix>auth`.
    pub branch_prefix: String,
    /// How a green branch lands back on base.
    pub merge: MergeMode,
    /// The default agent tool; a per-task `tool:` overrides it.
    pub agent_tool: String,
    /// The default model forwarded to the agent CLI; `None` lets the CLI use its
    /// own default. A task's / workflow's model wins over this.
    pub agent_model: Option<String>,
    /// The default effort forwarded to the agent CLI; `None` lets the CLI use its
    /// own default. A task's / workflow's effort wins over this.
    pub agent_effort: Option<String>,
    /// Extra CLI flags forwarded per agent tool so a headless run never stalls on
    /// an interactive approval gate. Keyed by tool id (`claude`, `codex`, …); a
    /// tool with no entry gets none.
    ///
    /// Defaults `claude` → `--permission-mode auto`: a classifier auto-approves
    /// each action instead of prompting, so the agent does the work and never
    /// hangs. NOT `--dangerously-skip-permissions` — in claude v2.1.x that always
    /// shows the non-headless-answerable "Bypass Permissions mode … Yes, I accept"
    /// consent screen (which `bypassPermissionsModeAccepted` does not suppress),
    /// parking the agent `launch_blocked`. `auto` mode has no such screen.
    pub permission_flags: HashMap<String, Vec<String>>,
    /// Pre-seed Claude Code's per-folder trust flag (`hasTrustDialogAccepted`) in
    /// `~/.claude.json` before launching a `claude` agent, so a headless run in a
    /// never-trusted worktree doesn't freeze on the interactive *"Yes, I trust
    /// this folder"* dialog (a gate distinct from the per-tool allow-list). On by
    /// default; a task can override it. Only ever touches the launch dir's own
    /// `projects.<path>` entry, never the global bypass-mode flag.
    pub auto_trust_agent_folder: bool,
    /// A running task whose agent is gone and whose heartbeat is older than this
    /// is reclaimed to `ready` on the next tick.
    pub stale_after_secs: u64,
    /// How often the supervisor loop ticks.
    pub tick_secs: u64,
    /// Reverse issue→task sync cadence: run the GitHub issue-state poll every
    /// Nth tick, so a coarse cadence keeps the extra `gh` calls cheap. `0`
    /// disables the reverse poll entirely; `1` runs it every tick.
    pub issue_sync_every_n_ticks: u64,
}

/// The subset of `lazybones.yaml` the scheduler reads; every key optional.
#[derive(Debug, Default, Deserialize)]
struct File {
    target_repo: Option<String>,
    base_branch: Option<String>,
    remote: Option<String>,
    gate: Option<Vec<String>>,
    concurrency: Option<usize>,
    worktrees: Option<bool>,
    worktree_root: Option<String>,
    branch_prefix: Option<String>,
    merge: Option<String>,
    agent_tool: Option<String>,
    agent_model: Option<String>,
    agent_effort: Option<String>,
    permission_flags: Option<HashMap<String, Vec<String>>>,
    auto_trust_agent_folder: Option<bool>,
    stale_after_secs: Option<u64>,
    tick_secs: Option<u64>,
    issue_sync_every_n_ticks: Option<u64>,
}

impl EngineConfig {
    /// Load from `path` (if present) then apply `LAZYBONES_*` env overrides.
    ///
    /// A missing file is fine: defaults plus env cover a headless boot, exactly
    /// like the boot [`Config`](../../lazybones-cli/src/configure.rs).
    ///
    /// # Errors
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let file: File = if path.exists() {
            serde_yaml::from_str(&std::fs::read_to_string(path)?)?
        } else {
            File::default()
        };

        Ok(Self {
            target_repo: PathBuf::from(env_or("LAZYBONES_TARGET_REPO", file.target_repo, ".")),
            base_branch: env_or("LAZYBONES_BASE_BRANCH", file.base_branch, "main"),
            remote: env_or("LAZYBONES_REMOTE", file.remote, "origin"),
            gate: gate(file.gate),
            concurrency: env_num("LAZYBONES_CONCURRENCY", file.concurrency, 3),
            worktrees: env_bool("LAZYBONES_WORKTREES", file.worktrees, true),
            worktree_root: env_or("LAZYBONES_WORKTREE_ROOT", file.worktree_root, ".lazy/wt"),
            branch_prefix: env_or("LAZYBONES_BRANCH_PREFIX", file.branch_prefix, "lazy/"),
            merge: MergeMode::parse(&env_or("LAZYBONES_MERGE", file.merge, "fast-forward")),
            agent_tool: env_or("LAZYBONES_AGENT_TOOL", file.agent_tool, "claude"),
            agent_model: env_opt("LAZYBONES_AGENT_MODEL", file.agent_model),
            agent_effort: env_opt("LAZYBONES_AGENT_EFFORT", file.agent_effort),
            permission_flags: permission_flags(file.permission_flags),
            auto_trust_agent_folder: env_bool(
                "LAZYBONES_AUTO_TRUST_AGENT_FOLDER",
                file.auto_trust_agent_folder,
                true,
            ),
            stale_after_secs: env_num("LAZYBONES_STALE_AFTER_SECS", file.stale_after_secs, 300),
            tick_secs: env_num("LAZYBONES_TICK_SECS", file.tick_secs, 2),
            issue_sync_every_n_ticks: env_num(
                "LAZYBONES_ISSUE_SYNC_EVERY_N_TICKS",
                file.issue_sync_every_n_ticks,
                30,
            ),
        })
    }
}

/// Resolve the gate: env (newline-separated) wins over the file list, then the
/// rubix defaults — matching the example `lazybones.yaml`.
fn gate(file: Option<Vec<String>>) -> Vec<String> {
    if let Ok(env) = std::env::var("LAZYBONES_GATE") {
        return split_gate_env(&env);
    }
    file.unwrap_or_else(|| {
        vec![
            "cargo test --workspace".to_owned(),
            "cargo clippy --workspace --all-targets -- -D warnings".to_owned(),
        ]
    })
}

/// Resolve the per-tool permission-bypass flags: the file map if present, else
/// the baked-in default that lets a headless `claude` clear its trust + approval
/// gates. A file value (even an empty map) replaces the default wholesale, so an
/// operator can opt a tool out by giving it `[]` or omitting its key.
fn permission_flags(file: Option<HashMap<String, Vec<String>>>) -> HashMap<String, Vec<String>> {
    file.unwrap_or_else(|| {
        HashMap::from([(
            "claude".to_owned(),
            vec!["--permission-mode".to_owned(), "auto".to_owned()],
        )])
    })
}

/// Split a newline-separated `LAZYBONES_GATE` value into trimmed, non-empty
/// commands (the env transport for the gate list).
fn split_gate_env(env: &str) -> Vec<String> {
    env.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Resolve a string setting: env var wins, then the file value, then `default`.
fn env_or(var: &str, file: Option<String>, default: &str) -> String {
    std::env::var(var)
        .ok()
        .or(file)
        .unwrap_or_else(|| default.to_owned())
}

/// Resolve an optional string setting: env var wins, then the file value, else
/// `None` (no default — the agent CLI applies its own).
fn env_opt(var: &str, file: Option<String>) -> Option<String> {
    std::env::var(var).ok().or(file)
}

/// Resolve a numeric setting; an unparseable env value falls through to file/default.
fn env_num<T: std::str::FromStr + Copy>(var: &str, file: Option<T>, default: T) -> T {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .or(file)
        .unwrap_or(default)
}

/// Resolve a boolean setting; an unparseable env value falls through to file/default.
fn env_bool(var: &str, file: Option<bool>, default: bool) -> bool {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .or(file)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The workspace forbids `unsafe`, so these tests never mutate the process
    // environment. The env > file > default precedence is exercised by the
    // pure helpers (`split_gate_env`) and by the documented `env_or` ordering;
    // the file > default path is exercised end-to-end through `load`.

    #[test]
    fn defaults_when_file_absent() {
        let cfg = EngineConfig::load(Path::new("/no/such/lazybones-engine-test.yaml")).unwrap();
        assert_eq!(cfg.base_branch, "main");
        assert_eq!(cfg.remote, "origin");
        assert_eq!(cfg.concurrency, 3);
        assert!(cfg.worktrees);
        assert_eq!(cfg.merge, MergeMode::FastForward);
        assert_eq!(cfg.gate.len(), 2);
        // Folder-trust auto-seeding is on by default so a fresh worktree never
        // freezes on the trust dialog without the operator opting in.
        assert!(cfg.auto_trust_agent_folder);
    }

    #[test]
    fn auto_trust_agent_folder_file_can_disable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lazybones.yaml");
        std::fs::write(&path, "auto_trust_agent_folder: false\n").unwrap();
        let cfg = EngineConfig::load(&path).unwrap();
        assert!(!cfg.auto_trust_agent_folder);
    }

    #[test]
    fn file_overrides_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lazybones.yaml");
        std::fs::write(
            &path,
            "base_branch: dev\nconcurrency: 5\nworktrees: false\nmerge: merge\ngate:\n  - cargo check\n",
        )
        .unwrap();
        let cfg = EngineConfig::load(&path).unwrap();
        assert_eq!(cfg.base_branch, "dev");
        assert_eq!(cfg.concurrency, 5);
        assert!(!cfg.worktrees);
        assert_eq!(cfg.merge, MergeMode::Merge);
        assert_eq!(cfg.gate, vec!["cargo check".to_owned()]);
    }

    #[test]
    fn gate_env_splits_on_newlines_trimming_blanks() {
        assert_eq!(
            split_gate_env("  cargo test  \n\n cargo clippy \n"),
            vec!["cargo test".to_owned(), "cargo clippy".to_owned()]
        );
    }

    #[test]
    fn permission_flags_default_uses_claude_auto_mode() {
        let cfg = EngineConfig::load(Path::new("/no/such/lazybones-engine-test.yaml")).unwrap();
        // `--permission-mode auto`: classifier auto-approves, never the
        // non-headless-answerable bypass-mode consent screen.
        assert_eq!(
            cfg.permission_flags.get("claude").map(Vec::as_slice),
            Some(&["--permission-mode".to_owned(), "auto".to_owned()][..])
        );
        // No mapping for other tools by default.
        assert!(!cfg.permission_flags.contains_key("codex"));
    }

    #[test]
    fn permission_flags_file_replaces_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lazybones.yaml");
        std::fs::write(&path, "permission_flags:\n  codex:\n    - --yolo\n").unwrap();
        let cfg = EngineConfig::load(&path).unwrap();
        // A file map replaces the baked-in default wholesale.
        assert!(!cfg.permission_flags.contains_key("claude"));
        assert_eq!(
            cfg.permission_flags.get("codex").map(Vec::as_slice),
            Some(&["--yolo".to_owned()][..])
        );
    }

    #[test]
    fn merge_mode_parses_known_and_defaults_unknown() {
        assert_eq!(MergeMode::parse("merge"), MergeMode::Merge);
        assert_eq!(MergeMode::parse("pr"), MergeMode::Pr);
        assert_eq!(MergeMode::parse("fast-forward"), MergeMode::FastForward);
        assert_eq!(MergeMode::parse("garbage"), MergeMode::FastForward);
    }
}
