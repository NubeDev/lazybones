//! The MCP tool surface, grouped to mirror the four capability groups of the
//! design (docs/mcp/README.md §6). Tools are named `<group>.<verb>` so a client
//! lists them grouped; reads need no capability, every mutator names the capability
//! it checks via [`crate::auth::require`].
//!
//! Each tool is a thin method: deserialize typed args ([`crate::args`]) → guard the
//! capability → call the existing [`StoreHandle`](lazybones_store::StoreHandle) /
//! engine verb → serialize the existing domain type as the result. **No business
//! logic lives here** — it is the REST handlers' twin over the same store boundary,
//! so the two surfaces can never drift.
//!
//! Each group contributes its own `#[tool_router(router = …)]` block over
//! [`McpServer`](crate::server::McpServer); [`router`] merges them into the one
//! [`ToolRouter`](crate::ToolRouter) the server holds. P0 (task `mcp-spike`)
//! wires `supervise::state.health` + `orchestrate::workflow.create`; the remaining
//! groups register their verbs here as they land.

pub mod documents;
pub mod extensions;
pub mod orchestrate;
pub mod supervise;

use rmcp::handler::server::wrapper::Json;
use serde::Serialize;
use serde_json::Value;

use crate::ToolRouter;
use crate::error::{McpError, McpResult};
use crate::server::McpServer;

/// Assemble the full MCP tool surface by merging each group's router. The server's
/// `new()` stores the result; adding a group is adding its router to this sum.
#[must_use]
pub(crate) fn router() -> ToolRouter<McpServer> {
    McpServer::supervise_router()
        + McpServer::orchestrate_router()
        + McpServer::documents_router()
        + McpServer::extensions_router()
}

/// Serialize an existing domain type as a tool's JSON result — the shared tail of
/// every tool that returns a store/engine value. We hand back `Json<Value>` (rather
/// than the typed `Json<T>`) so the wire shape is the domain type's own serde form,
/// identical to what the REST route returns, with no second schema to drift.
///
/// A serialization failure is an internal error (a domain type that won't serialize
/// is a bug, not a client fault).
pub(crate) fn json<T: Serialize>(value: T) -> McpResult<Json<Value>> {
    let value =
        serde_json::to_value(value).map_err(|e| McpError::Internal(format!("serialize: {e}")))?;
    Ok(Json(value))
}
