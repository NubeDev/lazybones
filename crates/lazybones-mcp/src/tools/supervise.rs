//! Supervision / read tools — `state.health`, `state.engine`, `state.agents`,
//! `run.history`, `run.follow_ups`, `task.hcom_log`, `task.transcript`,
//! `run.hcom_log` (design §6.4).
//!
//! These mirror the open REST reads so any agent can answer "what is the state of
//! X?" without a token — no capability required. MCP has no first-class
//! server-push in v1; these are request/response snapshots and the existing SSE
//! `GET /stream` stays the realtime channel.

use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use serde_json::{Value, json};

use lazybones_store::{FollowUpFilter, HcomLogFilter};

use crate::args::{IdArgs, RunReadArgs, TaskHcomLogArgs};
use crate::error::{McpError, McpResult};
use crate::server::McpServer;
use crate::tools::json as to_json;

#[tool_router(router = supervise_router, vis = "pub(crate)")]
impl McpServer {
    /// `state.health` — probe the store and report process liveness. The twin of
    /// `GET /health`: no capability, no token required (design §6.4). Returns
    /// `{"status":"ok"}` when the store answers, `{"status":"unavailable"}` otherwise
    /// — the same body the REST route serves (just never a transport-level error, so
    /// a client always gets a readable status).
    #[tool(
        name = "state.health",
        description = "Liveness of the lazybones process and its store. No capability required (twin of GET /health). Returns {\"status\":\"ok\"} when the store answers, {\"status\":\"unavailable\"} otherwise."
    )]
    pub async fn state_health(&self) -> McpResult<Json<Value>> {
        let status = match self.store().health().await {
            Ok(()) => "ok",
            Err(_) => "unavailable",
        };
        Ok(Json(json!({ "status": status })))
    }

    /// `state.engine` — availability of the hcom orchestration engine. The twin of
    /// `GET /engine`: an unguarded PATH + `--version` probe that reveals only whether
    /// hcom is installed and its version (design §6.4).
    #[tool(
        name = "state.engine",
        description = "Availability of the hcom orchestration engine: whether the hcom binary is on PATH and its version. No capability required (twin of GET /engine)."
    )]
    pub async fn state_engine(&self) -> McpResult<Json<Value>> {
        let installed = hcom_on_path();
        let version = if installed { hcom_version() } else { None };
        Ok(Json(json!({
            "engine": "hcom",
            "installed": installed,
            "version": version,
            "install_hint": "install hcom (the orchestration engine), then `hcom status`",
        })))
    }

    /// `state.agents` — the configured agent catalog (which agent CLIs lazybones
    /// knows how to run). A store read over the durable catalog — no capability and
    /// no secrets (the REST `/agents` route's credential-aware view is loop-only and
    /// stays REST-side; design §6.4 exposes only the open catalog over MCP).
    #[tool(
        name = "state.agents",
        description = "The configured agent catalog (which agent CLIs lazybones knows how to run). No capability required; reveals no secrets."
    )]
    pub async fn state_agents(&self) -> McpResult<Json<Value>> {
        let agents = self.store().list_agents().await.map_err(McpError::from)?;
        to_json(agents)
    }

    /// `run.history` — every recorded transition for a run, oldest first. An open
    /// read, the twin of `GET /runs/:id`.
    #[tool(
        name = "run.history",
        description = "Every recorded transition for a run, oldest first. No capability required (twin of GET /runs/:id)."
    )]
    pub async fn run_history(
        &self,
        Parameters(args): Parameters<RunReadArgs>,
    ) -> McpResult<Json<Value>> {
        let events = self
            .store()
            .run_history(&args.run)
            .await
            .map_err(McpError::from)?;
        to_json(events)
    }

    /// `run.follow_ups` — a run's follow-ups, freshest first, optionally filtered by
    /// `status`/`task`. An open read, the twin of `GET /runs/:id/follow-ups`.
    #[tool(
        name = "run.follow_ups",
        description = "A run's follow-ups (the durable 'a human needs to act' surface), optionally filtered by status (open|resolved) or task. No capability required (twin of GET /runs/:id/follow-ups)."
    )]
    pub async fn run_follow_ups(
        &self,
        Parameters(args): Parameters<RunReadArgs>,
    ) -> McpResult<Json<Value>> {
        let filter = FollowUpFilter {
            status: args.status,
            task: args.task,
        };
        let items = self
            .store()
            .run_follow_ups(&args.run, &filter)
            .await
            .map_err(McpError::from)?;
        to_json(items)
    }

    /// `run.hcom_log` — a run's raw agent log (the fabric's record), oldest first,
    /// with the page filters. An open read, the twin of `GET /runs/:id/hcom`.
    #[tool(
        name = "run.hcom_log",
        description = "A run's raw agent log (what hcom saw the agents do/say), oldest first, with optional task/kind/after/limit filters. No capability required (twin of GET /runs/:id/hcom)."
    )]
    pub async fn run_hcom_log(
        &self,
        Parameters(args): Parameters<RunReadArgs>,
    ) -> McpResult<Json<Value>> {
        let filter = HcomLogFilter {
            task: args.task,
            kind: args.kind,
            after: args.after,
            limit: args.limit,
        };
        let entries = self
            .store()
            .run_hcom_log(&args.run, &filter)
            .await
            .map_err(McpError::from)?;
        to_json(entries)
    }

    /// `task.hcom_log` — one task's full agent trace. Sugar for `run.hcom_log`
    /// filtered to the task, resolving its `run_id` first. The twin of
    /// `GET /tasks/:id/hcom`. `404` if the task is unknown.
    #[tool(
        name = "task.hcom_log",
        description = "One task's full raw agent trace, with optional kind/after/limit filters. No capability required (twin of GET /tasks/:id/hcom)."
    )]
    pub async fn task_hcom_log(
        &self,
        Parameters(args): Parameters<TaskHcomLogArgs>,
    ) -> McpResult<Json<Value>> {
        let task = self
            .store()
            .get_task(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        // The hcom log is keyed by the workflow `run_id`, not the dotted event-
        // grouping `run` label; fall back to `run` only for a standalone task.
        let run = task.run_id.as_deref().unwrap_or(&task.run);
        let filter = HcomLogFilter {
            task: Some(args.id.clone()),
            kind: args.kind,
            after: args.after,
            limit: args.limit,
        };
        let entries = self
            .store()
            .run_hcom_log(run, &filter)
            .await
            .map_err(McpError::from)?;
        to_json(entries)
    }

    /// `task.transcript` — the deep hcom transcript for a task's agent (tool I/O,
    /// file edits, prose). An on-demand passthrough to `hcom transcript`, the twin of
    /// `GET /tasks/:id/transcript`. Not a stored artifact — it only works while hcom
    /// still retains the agent. `404` if the task is unknown or was never claimed.
    #[tool(
        name = "task.transcript",
        description = "The deep hcom transcript for a task's agent (tool I/O, file edits, prose), fetched on demand. No capability required (twin of GET /tasks/:id/transcript). Only works while hcom still retains the agent."
    )]
    pub async fn task_transcript(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        let task = self
            .store()
            .get_task(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        let agent = task.session.ok_or_else(|| {
            McpError::bad_request(format!(
                "task `{}` has no agent session (never claimed)",
                args.id
            ))
        })?;

        let bin = std::env::var("HCOM_BIN").unwrap_or_else(|_| "hcom".to_owned());
        let mut cmd = tokio::process::Command::new(&bin);
        cmd.arg("transcript")
            .arg(&agent)
            .arg("--json")
            .arg("--full");
        if let Some(dir) = std::env::var_os("HCOM_DIR") {
            cmd.env("HCOM_DIR", dir);
        }

        let out = cmd.output().await.map_err(|e| {
            McpError::Internal(format!("could not launch hcom transcript: {e}"))
        })?;
        if !out.status.success() {
            return Err(McpError::bad_request(format!(
                "hcom transcript for agent `{agent}` failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ))
            .into());
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let value: Value = serde_json::from_str(stdout.trim())
            .map_err(|e| McpError::Internal(format!("hcom transcript returned non-JSON: {e}")))?;
        Ok(Json(value))
    }
}

/// Whether the `hcom` binary is reachable on `PATH` — a cheap existence probe, the
/// MCP twin of the REST engine module's `on_path("hcom")`.
fn hcom_on_path() -> bool {
    let bin = std::env::var("HCOM_BIN").unwrap_or_else(|_| "hcom".to_owned());
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(&bin).is_file())
}

/// `hcom --version`, trimmed, if it ran — the MCP twin of `version_of("hcom")`.
fn hcom_version() -> Option<String> {
    let bin = std::env::var("HCOM_BIN").unwrap_or_else(|_| "hcom".to_owned());
    let out = std::process::Command::new(bin).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!v.is_empty()).then_some(v)
}
