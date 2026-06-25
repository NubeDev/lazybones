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

use crate::ToolRouter;
use crate::server::McpServer;

/// Assemble the full MCP tool surface by merging each group's router. The server's
/// `new()` stores the result; adding a group is adding its router to this sum.
#[must_use]
pub(crate) fn router() -> ToolRouter<McpServer> {
    McpServer::supervise_router() + McpServer::orchestrate_router()
}
