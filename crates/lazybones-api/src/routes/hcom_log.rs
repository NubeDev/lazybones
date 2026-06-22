//! `GET /runs/:id/hcom` + `GET /tasks/:id/hcom` — the durable raw agent log.
//!
//! The **fabric's** record (what hcom saw the agent do/say), alongside the run
//! log's **brain's** record (`GET /runs/:id`). Reads the durable `hcom_log` table,
//! oldest first; unauthenticated like `/runs/:id`, since it only replays history a
//! local operator can already see (docs/hcom-logs-scope.md).

use axum::Json;
use axum::extract::{Path, Query, State};
use lazybones_store::{HcomLogEntry, HcomLogFilter, StoreError};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

/// Query filters shared by both routes: `?task=`, `?kind=`, `?after=`, `?limit=`.
#[derive(Debug, Default, Deserialize)]
pub struct HcomLogQuery {
    /// Restrict to one task's agent.
    pub task: Option<String>,
    /// Restrict to one event kind (`message | status | life`).
    pub kind: Option<String>,
    /// Page boundary: only events with `hcom_id > after`.
    pub after: Option<i64>,
    /// Page size cap.
    pub limit: Option<usize>,
}

impl HcomLogQuery {
    fn into_filter(self) -> HcomLogFilter {
        HcomLogFilter {
            task: self.task,
            kind: self.kind,
            after: self.after,
            limit: self.limit,
        }
    }
}

/// `GET /runs/:id/hcom` — the run's raw agent log, oldest first.
pub async fn run_hcom_log(
    State(state): State<AppState>,
    Path(run): Path<String>,
    Query(query): Query<HcomLogQuery>,
) -> ApiResult<Json<Vec<HcomLogEntry>>> {
    let entries = state.store.run_hcom_log(&run, &query.into_filter()).await?;
    Ok(Json(entries))
}

/// `GET /tasks/:id/hcom` — sugar for `GET /runs/:run/hcom?task=:id`: one agent's
/// full trace. Resolves the task's run, then filters to that task.
///
/// The hcom log is keyed by the workflow `run_id`, not the dotted event-grouping
/// `run` label (the tail in `hcom_tail.rs` resolves a tag to `(run_id, task)`, and
/// the workflow Logs tab seeds from `run_id`). A workflow task's `run` is a legacy
/// label like `lazybones-run` that does **not** match where its events landed, so
/// keying off `run` here returns an empty trace. Prefer `run_id`; fall back to
/// `run` only for a standalone task that has no `run_id`.
pub async fn task_hcom_log(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HcomLogQuery>,
) -> ApiResult<Json<Vec<HcomLogEntry>>> {
    let task = state
        .store
        .get_task(&id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.clone()))?;
    let run = task.run_id.as_deref().unwrap_or(&task.run);
    let mut filter = query.into_filter();
    filter.task = Some(id);
    let entries = state.store.run_hcom_log(run, &filter).await?;
    Ok(Json(entries))
}
