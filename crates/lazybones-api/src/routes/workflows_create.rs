//! `POST /workflows` — create a workflow bound to a workspace.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::{Run, Workspace};

use crate::dto::CreateWorkflowBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Create a workflow (an empty, `active` run). Requires `Author` (loop-only).
/// Returns the created run, or `409` if the id already exists.
pub async fn create_workflow(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateWorkflowBody>,
) -> ApiResult<Json<Run>> {
    session.require(Capability::Author, "author", &body.id)?;
    let workspace = Workspace {
        repo: body.workspace.repo,
        base_branch: body.workspace.base_branch,
        branch_prefix: body.workspace.branch_prefix,
        worktree_mode: body.workspace.worktree_mode,
    };
    let run = Run::new(&body.id, &body.title, workspace, state.store.now());
    Ok(Json(state.store.create_run(&run).await?))
}
