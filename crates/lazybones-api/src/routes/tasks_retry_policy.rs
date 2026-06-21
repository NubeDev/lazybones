//! `PUT /tasks/:id/auto-retry` — set (or clear) a task's hands-off retry policy.
//!
//! When a task has an auto-retry strategy, the scheduler re-attempts it on a block
//! — with that strategy's guidance folded into the re-spawn prompt — up to
//! `max_retries` times, instead of leaving it for a human (see
//! `scheduler::finish`). This route is how an operator turns that on/off and tunes
//! the cap; `strategy: null` turns it back off. It is durable config and never
//! touches lifecycle state. Requires `Block` (the operator task-control cap, like
//! cancel/retry). `404` if the task is unknown.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{RetryStrategy, StoreError, Task};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// The auto-retry policy to set. `strategy: null` (or omitted) turns auto-retry
/// off; `max_retries` omitted leaves the existing cap unchanged.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AutoRetryBody {
    /// The fix intent for each hands-off re-attempt (`long_term`/`quick`), or
    /// `null` to disable auto-retry.
    pub strategy: Option<RetryStrategy>,
    /// Cap on hands-off re-attempts before the task stays blocked for a human.
    /// `None` leaves the current cap unchanged.
    pub max_retries: Option<u32>,
}

/// Set task `:id`'s auto-retry policy. Returns the updated task. `404` if unknown.
pub async fn set_auto_retry(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    body: Option<Json<AutoRetryBody>>,
) -> ApiResult<Json<Task>> {
    session.require(Capability::Block, "block", &id)?;
    let Json(opts) = body.unwrap_or_default();

    // Surface a clear 404 for an unknown task before writing anything.
    state
        .store
        .get_task(&id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.clone()))?;

    let task = state
        .store
        .set_retry_policy(&id, opts.strategy, opts.max_retries)
        .await?;
    Ok(Json(task))
}
