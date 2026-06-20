//! `POST /workflows/:id/tasks` — add a task to a workflow.
//!
//! The task is linked to the workflow via `run_id` (the real relationship; the
//! dotted board label is derived, never parsed back). When `from_template` is
//! set the spec/tool/default-mode come from that template; the body's explicit
//! fields then refine the instantiated task. Deps are wired as graph edges,
//! mirroring the standalone authoring path.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{StoreError, Task, deps_with_reuse, instantiate};

use crate::dto::AddWorkflowTaskBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Add a task to workflow `:id`. Requires `Author` (loop-only). Returns the
/// created task; `404` if the workflow is unknown, `409` if the task id exists
/// or the named template is missing.
pub async fn add_workflow_task(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<AddWorkflowTaskBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Author, "author", &body.id)?;

    // The workflow must exist (404 otherwise) so its tasks key off a real run.
    state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;

    // Build the task: instantiate from a template, or author from the body.
    let mut task = match &body.from_template {
        Some(template_id) => {
            let template = state
                .store
                .get_template(template_id)
                .await?
                .ok_or(StoreError::TemplateNotFound(template_id.clone()))?;
            instantiate(&template, &body.id, &body.title, &state.run, &id, body.deps.clone())
        }
        None => {
            let mut t = Task::seed(
                &body.id,
                &state.run,
                &body.title,
                &body.spec,
                body.deps.clone(),
                Vec::new(),
                body.tool.clone(),
            );
            t.run_id = Some(id.clone());
            t
        }
    };

    // Refine with the explicit body fields (these win over template defaults).
    task.owns = body.owns.clone();
    if body.tool.is_some() {
        task.tool = body.tool.clone();
    }
    task.model = body.model.clone();
    task.effort = body.effort.clone();
    if body.worktree_mode_override.is_some() {
        task.worktree_mode_override = body.worktree_mode_override;
    }
    task.reuse_from = body.reuse_from.clone();

    // `reuse_from` implies a dependency on the source task: its worktree must
    // exist before this one claims it. When the source is a *known* task, fold it
    // into the dep set so the readiness graph orders them and the plan graph shows
    // the edge — one source of truth. When it's unknown here (a missing id, or a
    // task in another workflow), it stays out of the graph and the claim-time
    // `resolve_reuse` guard handles it (→ blocked with a reason), rather than
    // leaving the task wedged `pending` on a dep that this run never resolves.
    let source_known = match &body.reuse_from {
        Some(src) => state.store.get_task(src).await?.is_some(),
        None => false,
    };
    let deps = if source_known {
        deps_with_reuse(&body.deps, body.reuse_from.as_deref())
    } else {
        body.deps.clone()
    };
    task.deps = deps.clone();

    let created = state.store.create_task(&task).await?;
    for dep in &deps {
        state.store.relate_dep(&body.id, dep).await?;
    }
    Ok(Json(created))
}
