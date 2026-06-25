//! Orchestration tools — tasks, skills, templates, workflows (design §6.1).
//!
//! Authoring verbs (`workflow.create`/`add_task`, `task.create`/`update`,
//! `template.*`, `skill.*`) check `Capability::Author`; reads need none. Lifecycle
//! verbs (`workflow.start` → `Claim`; `workflow.stop`/`resume`/`restart` and
//! `task.retry`/`auto_retry`/`cancel` → `Block`) are present but gated, so the
//! default management (`Author`) token authors then hands back — it cannot start a
//! run. `follow_up.file` is the agent's "needs a human" escape hatch.
//!
//! P0 (task `mcp-spike`) lands one author tool,
//! [`workflow.create`](McpServer::workflow_create); the rest of §6.1 follows in
//! `mcp-orchestrate`.

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use serde_json::Value;

use lazybones_auth::Capability;
use lazybones_store::Run;

use crate::args::WorkflowCreateArgs;
use crate::auth::{self, authorization_header};
use crate::error::{McpError, McpResult};
use crate::server::McpServer;

#[tool_router(router = orchestrate_router, vis = "pub(crate)")]
impl McpServer {
    /// `workflow.create` — create a workflow (an empty, `active` run) bound to a
    /// workspace. The twin of `POST /workflows`: requires `Capability::Author`, the
    /// same gate the route checks (design §6.1). Authoring is not running — the run
    /// promotes nothing until the operator starts it.
    ///
    /// Refuses with `unauthorized` when the call carries no (or an unregistered)
    /// token, and with `forbidden` (missing `author`) when the token lacks the
    /// capability. Conflicts when the id is already taken.
    #[tool(
        name = "workflow.create",
        description = "Create a workflow (an empty, active run) bound to a workspace. Requires the Author capability (twin of POST /workflows). Authoring is not running: the run promotes nothing until an operator starts it."
    )]
    pub async fn workflow_create(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<WorkflowCreateArgs>,
    ) -> McpResult<Json<Value>> {
        // Authenticate exactly like the REST route: bearer token → session, then the
        // per-tool capability guard. No/unknown token ⇒ unauthorized (this is a
        // mutator, so the unauthenticated read path does not apply).
        let session = self
            .session_for(authorization_header(&parts))
            .ok_or(McpError::Unauthorized)?;
        auth::require(&session, Capability::Author)?;

        let run = Run::new(
            &args.id,
            &args.title,
            args.workspace.into_workspace(),
            self.store().now(),
        );
        let created = self.store().create_run(&run).await.map_err(McpError::from)?;
        Ok(Json(serde_json::to_value(created).map_err(|e| {
            McpError::Internal(format!("serialize run: {e}"))
        })?))
    }
}
