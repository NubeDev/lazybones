//! The wire shapes for `GET /engine` and `GET /agents`, plus their builders.
//!
//! `EngineReport` answers "is hcom available?" (the orchestration engine).
//! `AgentReport` answers "can this agent CLI run, and is it set up?" — joining
//! the static [`catalog`](super::catalog) against live detection and the set of
//! stored credentials.

use std::collections::HashSet;

use serde::Serialize;

use super::catalog::AGENTS;
use super::detect::{env_set, hcom_tools, on_path, probe_agent, version_of};

/// Availability of the hcom orchestration engine.
#[derive(Debug, Clone, Serialize)]
pub struct EngineReport {
    /// The engine name (always `hcom`).
    pub engine: &'static str,
    /// Whether the `hcom` binary is on PATH.
    pub installed: bool,
    /// `hcom --version`, if it ran.
    pub version: Option<String>,
    /// One-liner pointing at how to install when absent.
    pub install_hint: &'static str,
}

/// Setup state of one agent CLI.
#[derive(Debug, Clone, Serialize)]
pub struct AgentReport {
    /// The `agent_tool` id (matches hcom's `tools` key).
    pub tool: String,
    /// Human label.
    pub label: String,
    /// Whether the tool is installed, as reported by `hcom status --json`.
    pub installed: bool,
    /// Reserved; hcom doesn't report a per-tool version, so always `None`.
    pub version: Option<String>,
    /// The env var the CLI authenticates with.
    pub env_var: String,
    /// Whether a credential is stored for this tool in the secret store.
    pub key_stored: bool,
    /// Whether that env var is already present in the daemon's environment.
    pub key_in_env: bool,
    /// Whether the tool is ready to run a task (installed AND has a credential).
    pub ready: bool,
    /// How to obtain a credential / log in.
    pub login_hint: String,
}

/// Build the hcom availability report (cheap PATH + `--version` probe).
#[must_use]
pub fn engine_report() -> EngineReport {
    EngineReport {
        engine: "hcom",
        installed: on_path("hcom"),
        version: version_of("hcom"),
        install_hint: "install hcom (the orchestration engine), then `hcom status`",
    }
}

/// Result of a live credential test for one agent.
#[derive(Debug, Clone, Serialize)]
pub struct AgentTestResult {
    /// The tool that was tested.
    pub tool: String,
    /// Whether the agent authenticated and responded.
    pub ok: bool,
    /// Human-readable outcome (success summary or failure reason).
    pub detail: String,
}

/// How long a live probe may run before we give up (seconds).
const PROBE_TIMEOUT_SECS: u64 = 60;

/// Run a live credential probe for `tool`: launch the agent through hcom with its
/// credential and confirm it authenticates. `key` is the decrypted secret value
/// if one is stored; when `None`, the probe relies on whatever is already in the
/// daemon's environment (so `claude login`-style sessions still test true).
///
/// Returns `None` if `tool` is not a known agent (the route maps that to 404).
/// `key` is exported under the tool's catalog env var, so the route doesn't need
/// to know the var name — it just hands over the decrypted value (or `None`).
#[must_use]
pub fn test_agent(tool: &str, key: Option<&str>) -> Option<AgentTestResult> {
    let spec = AGENTS.iter().find(|s| s.tool == tool)?;
    let key_env = key.map(|v| (spec.env_var, v));
    let outcome = probe_agent(spec.tool, key_env, PROBE_TIMEOUT_SECS);
    Some(AgentTestResult {
        tool: spec.tool.to_owned(),
        ok: outcome.ok,
        detail: outcome.detail,
    })
}

/// The catalog env var a tool authenticates with, if `tool` is known. The route
/// uses this to pick the right decrypted secret out of the loop's env list.
#[must_use]
pub fn env_var_for(tool: &str) -> Option<&'static str> {
    AGENTS.iter().find(|s| s.tool == tool).map(|s| s.env_var)
}

/// Build the per-agent setup report. Install state comes from hcom (the engine
/// that launches agents) via a single `hcom status --json` probe, so editor-
/// bundled and snap-managed CLIs are detected the same way hcom will run them.
/// `stored` is the set of tool ids that have a credential in the secret store
/// (so the route stays the single DB caller).
#[must_use]
pub fn agent_reports(stored: &HashSet<String>) -> Vec<AgentReport> {
    let tools = hcom_tools();
    AGENTS
        .iter()
        .map(|spec| {
            let installed = tools.get(spec.tool).copied().unwrap_or(false);
            let key_stored = stored.contains(spec.tool);
            let key_in_env = env_set(spec.env_var);
            AgentReport {
                tool: spec.tool.to_owned(),
                label: spec.label.to_owned(),
                installed,
                version: None,
                env_var: spec.env_var.to_owned(),
                key_stored,
                key_in_env,
                ready: installed && (key_stored || key_in_env),
                login_hint: spec.login_hint.to_owned(),
            }
        })
        .collect()
}
