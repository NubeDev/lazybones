//! GitHub issue linkage for a task: create / link / unlink + the close-on-done
//! toggle. Thin handlers over the engine's `issue` actions (the engine owns the
//! `gh` plumbing; the UI never shells out itself). All `Author`-gated — wiring a
//! task to an issue is operator authoring of the queue.
//!
//! See docs/issue-linkage-scope.md.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{StoreError, Task};

use crate::dto::{CloseOnDoneBody, LinkIssueBody};
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// `POST /tasks/:id/issue` — create a GitHub issue from the task's title/spec
/// and link it. Rejects a standalone (run-less) task with a clear `400`.
pub async fn create_issue(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Author, "issue-create", &id)?;
    let task = lazybones_engine::issue_create(&state.store, &id).await?;
    Ok(Json(task))
}

/// `POST /tasks/:id/issue/link` — attach an existing issue by URL or `#number`,
/// validating it resolves on GitHub first.
pub async fn link_issue(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<LinkIssueBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Author, "issue-link", &id)?;
    let task = lazybones_engine::issue_link(&state.store, &id, &body.link).await?;
    Ok(Json(task))
}

/// `DELETE /tasks/:id/issue` — unlink the issue (clears the task's pointer only;
/// the GitHub issue is left untouched).
pub async fn unlink_issue(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Author, "issue-unlink", &id)?;
    let task = lazybones_engine::issue_unlink(&state.store, &id).await?;
    Ok(Json(task))
}

/// `PUT /tasks/:id/issue/close-on-done` — set whether reaching `done` closes the
/// linked issue. Requires the task to be linked (a policy with no issue is
/// meaningless → `404`).
pub async fn set_close_on_done(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<CloseOnDoneBody>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Author, "issue-close-policy", &id)?;
    let mut task = state
        .store
        .get_task(&id)
        .await?
        .ok_or(StoreError::TaskNotFound(id.clone()))?;
    if task.issue_url.is_none() {
        return Err(ApiError::NotFound);
    }
    task.issue_close_on_done = body.enabled;
    let task = state.store.set_issue_link(&task).await?;
    Ok(Json(task))
}
