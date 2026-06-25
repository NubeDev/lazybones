//! Supervision / read tools — `state.health`, `state.engine`, `state.agents`,
//! `run.history`, `run.follow_ups`, `task.hcom_log`, `task.transcript`,
//! `run.hcom_log` (design §6.4).
//!
//! These mirror the open REST reads so any agent can answer "what is the state of
//! X?" without a token — no capability required. MCP has no first-class
//! server-push in v1; these are request/response snapshots and the existing SSE
//! `GET /stream` stays the realtime channel.
//!
//! Scaffold: no tools yet (task `mcp-crate`); the §6.4 set lands in `mcp-spike` /
//! `mcp-orchestrate`.
