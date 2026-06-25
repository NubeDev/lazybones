//! Generic job-runner routes — the operator-facing surface of the job service.
//!
//! - `GET  /jobs` — the names of every registered job.
//! - `POST /jobs/:name` — run one job synchronously and return its report.
//!
//! Content sync registers `content-sync-pull` / `content-sync-push` here, but the
//! surface is generic so any future job (a Google Drive flavour, a cleanup task)
//! is runnable the same way with no new route. Running a job requires `Author`.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_jobs::JobReport;

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// `GET /jobs` — the registered job names (sorted).
pub async fn list_jobs(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(state.runner().registry().names())
}

/// `POST /jobs/:name` — run the named job to completion and return its report.
/// Requires `Author`; an unknown name is a `404`.
pub async fn run_job(
    State(state): State<AppState>,
    session: Session,
    Path(name): Path<String>,
) -> ApiResult<Json<JobReport>> {
    session.require(Capability::Author, "run-job", &name)?;
    let report = state.runner().run_now(&name).await?;
    Ok(Json(report))
}
