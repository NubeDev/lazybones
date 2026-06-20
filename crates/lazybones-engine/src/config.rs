//! Engine config: the scheduler keys from `lazybones.yaml` + `LAZYBONES_*` env.
//!
//! `configure.rs` in the CLI loads only the *boot* keys (bind, DB location). The
//! scheduler is the daemon now, so it must load the rest: the gate, concurrency,
//! worktree layout, and merge strategy. Same precedence as the boot config —
//! env wins over the file wins over the baked-in defaults.

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
    /// A running task whose agent is gone and whose heartbeat is older than this
    /// is reclaimed to `ready` on the next tick.
    pub stale_after_secs: u64,
    /// How often the supervisor loop ticks.
    pub tick_secs: u64,
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
    stale_after_secs: Option<u64>,
    tick_secs: Option<u64>,
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
            target_repo: PathBuf::from(env_or(
                "LAZYBONES_TARGET_REPO",
                file.target_repo,
                ".",
            )),
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
            stale_after_secs: env_num("LAZYBONES_STALE_AFTER_SECS", file.stale_after_secs, 300),
            tick_secs: env_num("LAZYBONES_TICK_SECS", file.tick_secs, 2),
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
    fn merge_mode_parses_known_and_defaults_unknown() {
        assert_eq!(MergeMode::parse("merge"), MergeMode::Merge);
        assert_eq!(MergeMode::parse("pr"), MergeMode::Pr);
        assert_eq!(MergeMode::parse("fast-forward"), MergeMode::FastForward);
        assert_eq!(MergeMode::parse("garbage"), MergeMode::FastForward);
    }
}
