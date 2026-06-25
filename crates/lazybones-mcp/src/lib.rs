//! lazybones-mcp — the [Model Context Protocol](https://modelcontextprotocol.io)
//! front door onto lazybones.
//!
//! This crate exposes lazybones over MCP so the in-app management agent *and* any
//! external agent (Claude Desktop, the `claude` CLI, Cursor, a custom rmcp client)
//! can drive lazybones through **typed MCP tools** instead of hand-rolled `curl`
//! calls. It is **not a new capability plane**: every tool is a thin twin of an
//! existing REST handler, gated by the *same* [`Capability`](lazybones_auth::Capability)
//! and [`ScopedSession`](lazybones_auth::ScopedSession) the routes use, calling the
//! *same* [`StoreHandle`](lazybones_store::StoreHandle) and engine handles directly
//! (no HTTP-to-self). Its blast radius equals its token's grant — nothing new is
//! reachable that a bearer token couldn't already reach over HTTP.
//!
//! See [`docs/mcp/README.md`](https://github.com/NubeDev/lazybones/blob/master/docs/mcp/README.md)
//! for the full design, capability mapping, and house rules.
//!
//! # House rules (carried verbatim from the design)
//!
//! - **Authoring is not running.** A session may freely *create* tasks / workflows
//!   / templates / skills / documents / extensions; *starting, stopping, retrying,
//!   deleting, installing-and-granting* are gated behind grants the default
//!   management token lacks.
//! - **No new privilege.** Loop-only `Capability::Extension`/`Capability::Secret`
//!   stay loop-only; secrets are never exposed as a tool at all.
//! - **No HTTP-to-self.** Tools call the store/engine in-process — they are a typed
//!   mirror of the REST routes, not a reimplementation of the domain logic.
//!
//! # Scaffold status
//!
//! The crate exposes an **empty** [`ToolRouter`] plus a
//! [`ServerHandler`](rmcp::ServerHandler) advertising name/version/instructions, and
//! [`streamable_http_service`] — the in-process service `lazybones-api` mounts at
//! `/mcp` with the bearer-token [`SessionResolver`] wired in (task `mcp-mount`). The
//! tool surface (§6) lands in follow-up tasks.

pub mod args;
pub mod auth;
pub mod error;
pub mod server;
pub mod tools;

use std::sync::Arc;

use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use lazybones_store::{BlobStore, StoreHandle};

pub use auth::SessionResolver;
pub use error::{McpError, McpResult};
pub use server::McpServer;

#[doc(no_inline)]
pub use rmcp::handler::server::router::tool::ToolRouter;

/// Build the in-process [`StreamableHttpService`] that `lazybones-api` mounts as an
/// axum sub-router at `/mcp` (design §4.1).
///
/// The service factory hands every MCP session its own [`McpServer`] over the shared
/// `store`, asset blob store, and bearer-token `resolver`, so each connection
/// authenticates through the **same** token → session registry a REST request does
/// and calls the **same** store boundary in-process — no HTTP-to-self (design
/// §2.1/§3). The `assets` blob store is shared with the REST surface so
/// `document.render` resolves logo/image bytes through the same backend. Sessions are
/// held in rmcp's in-memory [`LocalSessionManager`]; the caller layers the existing
/// `cors_layer()` + body limit over the mount.
#[must_use]
pub fn streamable_http_service(
    store: StoreHandle,
    assets: Arc<dyn BlobStore>,
    resolver: Arc<dyn SessionResolver>,
) -> StreamableHttpService<McpServer, LocalSessionManager> {
    StreamableHttpService::new(
        move || Ok(McpServer::new(store.clone(), resolver.clone()).with_assets(assets.clone())),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    )
}
