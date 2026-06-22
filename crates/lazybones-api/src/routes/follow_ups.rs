//! Follow-ups — the durable "a human needs to act" surface.
//!
//! - `GET /runs/:id/follow-ups` — a run's follow-ups (read-only, like the run log).
//! - `POST /follow-ups` — file one (an agent flags something; requires `Block`).
//! - `POST /follow-ups/:id/resolve` — clear one (operator action; requires `Author`).
//!
//! The scheduler files these automatically when it hits a wall it can't clear
//! (a consent screen, a spawn failure) — see
//! [`tick`](../../../lazybones-engine/src/scheduler/tick.rs). This module is the
//! REST surface an agent or the UI uses on top of the same store.

use axum::Json;
use axum::extract::{Path, Query, State};
use lazybones_auth::Capability;
use lazybones_store::{FollowUp, FollowUpFilter, NewFollowUpEntry};
use serde::Deserialize;

use crate::dto::FollowUpBody;
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// `?status=open&task=review` filters for the list route.
#[derive(Debug, Default, Deserialize)]
pub struct FollowUpQuery {
    /// Restrict to `open` or `resolved`; both when omitted.
    pub status: Option<String>,
    /// Restrict to one task's follow-ups.
    pub task: Option<String>,
}

/// `GET /runs/:id/follow-ups` — the run's follow-ups, freshest first. Read-only
/// and unauthenticated, like `GET /runs/:id` and the hcom log: it replays state a
/// local operator can already see.
pub async fn run_follow_ups(
    State(state): State<AppState>,
    Path(run): Path<String>,
    Query(query): Query<FollowUpQuery>,
) -> ApiResult<Json<Vec<FollowUp>>> {
    let filter = FollowUpFilter {
        status: query.status,
        task: query.task,
    };
    let items = state.store.run_follow_ups(&run, &filter).await?;
    Ok(Json(items))
}

/// `POST /follow-ups` — file a follow-up. Requires `Block`: an agent that can
/// block its own task can also flag something for a human. Idempotent on
/// `(run, dedup_key)`.
pub async fn file_follow_up(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<FollowUpBody>,
) -> ApiResult<Json<FollowUp>> {
    session.require(Capability::Block, "file follow-up", &body.run)?;
    let dedup_key = body.dedup_key.unwrap_or_else(|| body.title.clone());
    let entry = NewFollowUpEntry {
        run: body.run,
        task: body.task,
        dedup_key,
        kind: body.kind.unwrap_or_else(|| "note".to_owned()),
        title: body.title,
        detail: body.detail,
        actor: session.actor().to_owned(),
    };
    let filed = state.store.file_follow_up(entry).await?;
    Ok(Json(filed))
}

/// `POST /follow-ups/:id/resolve` — mark one resolved. Operator action; requires
/// `Author`. 404 if no follow-up has that id.
pub async fn resolve_follow_up(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<FollowUp>> {
    session.require(Capability::Author, "resolve follow-up", &id)?;
    let resolved = state
        .store
        .resolve_follow_up(&id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(resolved))
}
