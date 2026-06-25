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
//! Empty in this scaffold (task `mcp-crate`): the `#[tool]` methods register onto
//! [`McpServer`](crate::server::McpServer)'s [`ToolRouter`](crate::ToolRouter) as
//! each group is implemented.

pub mod documents;
pub mod extensions;
pub mod orchestrate;
pub mod supervise;
