//! `PATCH /tasks/:id` — overwrite a task's authored fields, reconcile deps.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{StoreError, Task, TaskEdit};

use crate::dto::UpdateTaskBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Update a task's authored fields (lifecycle preserved), then reconcile its
/// dependency edges against the previous set. Requires `Author` (loop-only).
/// Returns the updated task, or `404` if no such task exists.
pub async fn update_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<UpdateTaskBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Author, "author", &id)?;
    let old = state
        .store
        .get_task(&id)
        .await?
        .ok_or(StoreError::TaskNotFound(id.clone()))?;
    let updated = state
        .store
        .update_task(
            &id,
            TaskEdit {
                title: body.title,
                spec: body.spec,
                deps: body.deps.clone(),
                owns: body.owns,
                tool: body.tool,
                model: body.model,
                effort: body.effort,
                worktree_mode: body.worktree_mode,
                auto_trust_agent_folder: body.auto_trust_agent_folder,
                // The auto-retry policy is operator config (Block-guarded), set via
                // the retry route — not touched by this Author-guarded re-authoring.
                auto_retry: None,
                max_retries: None,
                // Close-on-done is issue config, managed by the dedicated issue
                // routes, not this authoring re-write.
                issue_close_on_done: None,
            },
        )
        .await?;
    for dep in &old.deps {
        if !body.deps.contains(dep) {
            state.store.unrelate_dep(&id, dep).await?;
        }
    }
    for dep in &body.deps {
        if !old.deps.contains(dep) {
            state.store.relate_dep(&id, dep).await?;
        }
    }
    Ok(Json(updated))
}
