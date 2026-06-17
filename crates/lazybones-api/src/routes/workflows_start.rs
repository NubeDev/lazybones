//! `POST /workflows/:id/start` — activate a workflow: promote eligible roots.
//!
//! "Start a workflow" is a control-plane action, not a claim (the UI never
//! claims — that is the scheduler's job, see docs/starting-workflows.md). It
//! stamps `started_at` and promotes every *eligible root* task — a `pending`
//! task in this workflow whose dependencies are all `done` — to `ready`, so the
//! in-process scheduler picks them up on its next tick. Requires `Claim`.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Status, StoreError, Transition};

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Activate workflow `:id` and promote its eligible root tasks. Returns the ids
/// promoted to `ready`. `404` if the workflow is unknown.
pub async fn start_workflow(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Claim, "claim", &id)?;

    state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;

    let now = state.store.now();
    state.store.mark_run_started(&id, &now).await?;

    let tasks = state.store.list_run_tasks(&id).await?;
    // Status by id so dependency readiness is checked within the workflow.
    let status_by_id: HashMap<&str, Status> =
        tasks.iter().map(|t| (t.id.as_str(), t.status)).collect();

    let mut promoted = Vec::new();
    for task in &tasks {
        if task.status != Status::Pending {
            continue;
        }
        // Eligible root: every dependency is `done` (an undone or unknown dep
        // means the scheduler promotes it later, once the dep lands).
        let ready = task
            .deps
            .iter()
            .all(|d| status_by_id.get(d.as_str()) == Some(&Status::Done));
        if !ready {
            continue;
        }
        match state
            .store
            .transition(&task.id, Transition::Ready, session.actor())
            .await
        {
            Ok(_) => promoted.push(task.id.clone()),
            Err(e) => tracing::warn!(task = %task.id, "start: promote failed: {e}"),
        }
    }

    Ok(Json(serde_json::json!({ "promoted": promoted })))
}
