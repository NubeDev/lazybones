//! `DELETE /workflows/:id` — hard-delete a workflow and its tasks.
//!
//! Distinct from `stop` (which keeps the record with `lifecycle=stopped`,
//! reversibly): delete is the real archive path — it removes the run row and
//! cascades to its tasks. It refuses (`409`) if any task is still live
//! (`running`/`gating`) so a delete can't orphan a worktree or leave an agent
//! running — stop first, then delete. Requires `Author` (loop-only), like the
//! other delete routes.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Status, StoreError};

use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// Delete workflow `:id` and its tasks. `404` if unknown, `409` if it has live
/// tasks. Returns whether it existed.
pub async fn delete_workflow(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Author, "author", &id)?;

    state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;

    // Refuse to delete a workflow with live work: a running/gating task owns a
    // worktree and a live agent, and a hard delete would orphan both. The
    // operator cancels first (which kills agents + blocks tasks), then deletes.
    let tasks = state.store.list_run_tasks(&id).await?;
    if let Some(live) = tasks
        .iter()
        .find(|t| matches!(t.status, Status::Running | Status::Gating))
    {
        return Err(ApiError::conflict(format!(
            "workflow {id} has a live task ({}, {:?}); cancel the workflow before deleting",
            live.id, live.status
        )));
    }

    let existed = state.store.delete_run(&id).await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}
