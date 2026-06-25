//! Supervision / read tools — `state.health`, `state.engine`, `state.agents`,
//! `run.history`, `run.follow_ups`, `task.hcom_log`, `task.transcript`,
//! `run.hcom_log` (design §6.4).
//!
//! These mirror the open REST reads so any agent can answer "what is the state of
//! X?" without a token — no capability required. MCP has no first-class
//! server-push in v1; these are request/response snapshots and the existing SSE
//! `GET /stream` stays the realtime channel.
//!
//! P0 (task `mcp-spike`) lands one read tool, [`state.health`](McpServer::state_health);
//! the rest of §6.4 follows in `mcp-orchestrate`.

use rmcp::handler::server::wrapper::Json;
use rmcp::{tool, tool_router};
use serde_json::{Value, json};

use crate::error::McpResult;
use crate::server::McpServer;

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
}
