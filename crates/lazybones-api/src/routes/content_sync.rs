//! Content-sync routes: report sync state and drive pull/push.
//!
//! - `GET  /content-sync/status` — where the local checkout stands vs the remote
//!   (drives the "you're out of sync — pull?" banner). Open read.
//! - `POST /content-sync/pull` — pull the remote + import into the store.
//! - `POST /content-sync/push` — export the store + commit + push.
//!
//! Pull and push run through the generic [`JobRunner`](lazybones_jobs::JobRunner)
//! (the `content-sync-pull` / `content-sync-push` jobs the daemon registers), so
//! the same code path is reachable both from here and as background jobs. Writes
//! require `Author`.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_engine::sync::{self, SyncStatus};
use lazybones_jobs::JobReport;

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// `GET /content-sync/status` — the current sync state (configured? ahead/behind?).
/// Never errors on a network problem — an unreachable remote reports
/// `state: "unknown"` so the UI stays quiet. Open read (local single-user daemon).
pub async fn get_status(State(state): State<AppState>) -> ApiResult<Json<SyncStatus>> {
    let status = sync::status(&state.store, state.data_dir()).await?;
    Ok(Json(status))
}

/// `POST /content-sync/pull` — pull the remote and import it. Requires `Author`.
pub async fn post_pull(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<JobReport>> {
    session.require(Capability::Author, "content-sync-pull", "content-sync")?;
    let report = state.runner().run_now(sync::PULL_JOB).await?;
    Ok(Json(report))
}

/// `POST /content-sync/push` — export the store and push it. Requires `Author`.
pub async fn post_push(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<JobReport>> {
    session.require(Capability::Author, "content-sync-push", "content-sync")?;
    let report = state.runner().run_now(sync::PUSH_JOB).await?;
    Ok(Json(report))
}
