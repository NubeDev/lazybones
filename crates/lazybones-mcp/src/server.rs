//! The MCP server: the `#[tool_handler]` struct that holds the shared state and
//! advertises the server's name / version / instructions.
//!
//! The struct carries the cloneable [`StoreHandle`] (and, in later tasks, the
//! engine handles) so every tool calls the durable store directly — the same store
//! boundary the REST handlers use, never an HTTP-to-self round trip (design §2.1).
//! The [`ToolRouter`] is assembled by the `#[tool_router]` macro; it is **empty** in
//! this scaffold and grows one `#[tool]` method per verb as the §6 surface lands.

use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool_handler};

use lazybones_auth::{Capability, ScopedSession};
use lazybones_store::StoreHandle;

use crate::auth::{self, SessionResolver};
use crate::error::McpError;

/// The run label MCP-authored standalone tasks group under when the daemon's
/// `LAZYBONES_RUN` is unset — the same default `lazybones-cli`'s `configure`
/// applies (`env_or("LAZYBONES_RUN", …, "lazybones-run")`), so the two surfaces
/// land their history in the same place.
const DEFAULT_RUN_LABEL: &str = "lazybones-run";

/// The instructions string advertised over `ServerHandler::get_info`. Distilled
/// from [`docs/managing-with-ai.md`](../../../docs/managing-with-ai.md) — the same
/// house rules the management runner folds into its system prompt — so MCP clients
/// get the operating contract without a separate cheat-sheet (design §7).
const INSTRUCTIONS: &str = "\
lazybones is a durable task queue + green-build gate with an in-process scheduler. \
You drive it through these MCP tools, which mirror its REST surface 1:1.

The split that matters:
- You are the CONTROL plane: you author and promote work; you never mark a task running yourself.
- The scheduler (a loop inside lazybonesd) is the EXECUTION plane: it promotes ready tasks, \
provisions worktrees, claims them, spawns the agent, runs the gate, and lands the branch.
A task runs only when both happen: you promote it AND the daemon is up.

House rules (settled defaults, not per-request choices):
- AUTHORING IS NOT RUNNING. Freely create workflows, tasks, templates, skills, documents, and \
extension sources. Do NOT start, stop, restart, retry, cancel, delete, or install/grant — those \
are gated behind capabilities the default management token does not hold and will refuse (403). \
Author the work, then hand back so the operator presses Start.
- A freshly authored workflow is `active` but has promoted nothing, so it sits idle and safe \
until the operator starts it.
- Permission mode is `auto` and daemon-global; there is no per-workflow/per-task bypass field. \
Do not try to set one — it is silently ignored.

Capabilities (your token's grant bounds everything):
- Read tools (state/logs/history) need no capability.
- Authoring tools need `Author` (tasks/templates/skills/workflows) or `Document` (documents/branding/assets).
- Lifecycle tools (workflow start/stop/resume/restart, task retry/cancel/auto-retry) need \
`Block`/`Claim`-class grants the Author profile lacks.
- Installing extensions, setting grants, and reading secrets are loop-only and never agent-reachable.

Status flow (lowercase): pending -> ready -> running -> gating -> done; any non-terminal state \
can go to blocked; a stale running task is reclaimed to ready. Revive a blocked task with a \
retry/chat (guided revive keeps the worktree and folds guidance into the re-spawn prompt). \
When you are stuck or need a human, file a follow-up.";

/// The MCP server handle. Cloneable so rmcp's session manager can hand each
/// connection its own clone over the shared store.
#[derive(Clone)]
pub struct McpServer {
    /// The durable store — every tool's path to lazybones' state. Shared, not forked
    /// (design §2.2: one store, in-process, no second source of truth).
    store: StoreHandle,
    /// Maps a request's bearer token to its [`ScopedSession`] against the API's token
    /// registry — the same map a REST request authenticates through (design §3). Held
    /// behind `Arc<dyn …>` so this crate stays free of a cycle back onto `lazybones-api`.
    resolver: Arc<dyn SessionResolver>,
    /// The event-grouping run label a standalone `task.create` seeds under — the
    /// twin of `AppState::run`, read from `LAZYBONES_RUN` (or [`DEFAULT_RUN_LABEL`])
    /// so MCP-authored tasks group exactly where the REST surface's do.
    run: String,
    /// The typed tool surface, assembled by [`crate::tools::router`] from each
    /// group's `#[tool_router]` block — the full §6.1 orchestration + §6.4
    /// supervision verbs.
    tool_router: ToolRouter<McpServer>,
}

impl McpServer {
    /// Build a server over the shared [`StoreHandle`] and the bearer-token
    /// [`SessionResolver`] the mount supplies. Later tasks extend the signature with
    /// the engine handles the orchestration/lifecycle tools need.
    #[must_use]
    pub fn new(store: StoreHandle, resolver: Arc<dyn SessionResolver>) -> Self {
        let run = std::env::var("LAZYBONES_RUN").unwrap_or_else(|_| DEFAULT_RUN_LABEL.to_owned());
        Self {
            store,
            resolver,
            run,
            tool_router: crate::tools::router(),
        }
    }

    /// The shared store handle the tool methods call. Exposed so the `tools::*`
    /// modules reach the store without re-plumbing it through each call.
    #[must_use]
    pub fn store(&self) -> &StoreHandle {
        &self.store
    }

    /// The run label a standalone `task.create` seeds under — the MCP twin of
    /// `AppState::run` (design §6.1), so an MCP-authored task groups its history
    /// exactly where a REST-authored one does.
    #[must_use]
    pub fn run_label(&self) -> &str {
        &self.run
    }

    /// Authenticate a mutating tool call and assert it carries `cap`, returning the
    /// resolved [`ScopedSession`] (so the tool can read `actor()` for the audit
    /// trail). The MCP twin of a REST handler's `session.require(cap, …)`: a
    /// missing/unknown token is [`Unauthorized`](McpError::Unauthorized); a token
    /// lacking `cap` is [`Forbidden`](McpError::Forbidden) (→ 403). The
    /// unauthenticated read path does **not** apply to mutators (design §3).
    ///
    /// # Errors
    ///
    /// [`McpError::Unauthorized`] with no resolvable session, or
    /// [`McpError::Forbidden`] when the session lacks `cap`.
    pub fn authorize(
        &self,
        authorization: Option<&str>,
        cap: Capability,
    ) -> Result<ScopedSession, McpError> {
        let session = self.session_for(authorization).ok_or(McpError::Unauthorized)?;
        auth::require(&session, cap)?;
        Ok(session)
    }

    /// Resolve the [`ScopedSession`] a tool call acts under from its `Authorization`
    /// header value (`None` ⇒ the unauthenticated read-only path).
    ///
    /// Tools read the header from the injected [`http::request::Parts`] (available via
    /// `RequestContext`/`Extension<Parts>`, see the rmcp transport docs) and pass it
    /// here; the token → session lookup runs against the API's registry through the
    /// [`SessionResolver`]. A mutator then asserts its capability via
    /// [`auth::require`](crate::auth::require) on the returned session.
    #[must_use]
    pub fn session_for(&self, authorization: Option<&str>) -> Option<ScopedSession> {
        auth::resolve_session(self.resolver.as_ref(), authorization)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        // `ServerInfo` (`InitializeResult`) is `#[non_exhaustive]`, so build it via
        // its constructor + `with_*` setters rather than a struct literal.
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(INSTRUCTIONS)
    }
}
