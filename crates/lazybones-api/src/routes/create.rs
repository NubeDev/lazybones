//! `POST /tasks` — author a new task and wire its dependency edges.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::Task;

use crate::dto::CreateTaskBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Create a task (status `Pending`), then relate each dependency. Requires
/// `Author` (loop-only). Returns the created task, or `409` if the id exists.
pub async fn create_task(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateTaskBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Author, "author", &body.id)?;
    let mut task = Task::seed(
        &body.id,
        &state.run,
        &body.title,
        &body.spec,
        body.deps.clone(),
        body.owns.clone(),
        body.tool.clone(),
    );
    task.worktree_mode = body.worktree_mode;
    let created = state.store.create_task(&task).await?;
    for dep in &body.deps {
        state.store.relate_dep(&body.id, dep).await?;
    }
    Ok(Json(created))
}
