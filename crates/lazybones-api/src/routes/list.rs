//! `GET /tasks` — list tasks, optionally filtered by `?status=ready`.

use axum::Json;
use axum::extract::{Query, State};
use lazybones_store::{Status, Task};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

/// Query string for the list route.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Narrow to one lifecycle status (e.g. `ready`).
    status: Option<String>,
}

/// List tasks. An unknown `status` value yields an empty list, not all tasks.
pub async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<Vec<Task>>> {
    match query.status.as_deref() {
        None => Ok(Json(state.store.list_tasks(None).await?)),
        Some(raw) => match parse_status(raw) {
            Some(status) => Ok(Json(state.store.list_tasks(Some(status)).await?)),
            None => Ok(Json(Vec::new())),
        },
    }
}

/// Map a query-string status to a [`Status`]; `None` for unknown values.
fn parse_status(s: &str) -> Option<Status> {
    match s {
        "pending" => Some(Status::Pending),
        "ready" => Some(Status::Ready),
        "running" => Some(Status::Running),
        "gating" => Some(Status::Gating),
        "done" => Some(Status::Done),
        "blocked" => Some(Status::Blocked),
        _ => None,
    }
}
