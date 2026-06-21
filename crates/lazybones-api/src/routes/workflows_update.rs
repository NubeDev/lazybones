//! `PATCH /workflows/:id` — edit a workflow's workspace defaults.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{StoreError, Workspace};

use crate::dto::{UpdateWorkflowBody, WorkflowDetail, WorkflowSummary};
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Overwrite a workflow's workspace defaults (the inheritable git + agent config:
/// base branch, branch prefix, worktree mode, tool/model/effort, gate, merge),
/// keeping its `repo`, lifecycle and timestamps. Tasks inherit the new defaults on
/// their next claim, so running work is undisturbed. Requires `Author`.
/// Returns the updated workflow detail, or `404` if no such workflow exists.
pub async fn update_workflow(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<UpdateWorkflowBody>,
) -> ApiResult<Json<WorkflowDetail>> {
    session.require(Capability::Author, "author", &id)?;
    let w = body.workspace;
    // `repo` is ignored by the store (the existing one is kept); pass it through.
    let workspace = Workspace {
        repo: w.repo,
        base_branch: w.base_branch,
        branch_prefix: w.branch_prefix,
        worktree_mode: w.worktree_mode,
        tool: w.tool,
        model: w.model,
        effort: w.effort,
        gate: w.gate,
        merge: w.merge,
    };
    let run = state
        .store
        .update_workspace(&id, workspace)
        .await
        .map_err(|e| match e {
            StoreError::RunNotFound(_) => StoreError::RunNotFound(id.clone()),
            other => other,
        })?;
    let tasks = state.store.list_run_tasks(&id).await?;
    let task_ids = tasks.iter().map(|t| t.id.clone()).collect();
    let summary = WorkflowSummary::new(run, &tasks);
    Ok(Json(WorkflowDetail { summary, task_ids }))
}
