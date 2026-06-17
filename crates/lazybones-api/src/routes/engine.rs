//! `GET /engine` — availability of the hcom orchestration engine.
//!
//! Unguarded, like `/health`: it reveals only whether hcom is installed and its
//! version, so the UI can show "engine ready" without a token.

use axum::Json;

use crate::engine::{EngineReport, engine_report};

/// Report whether hcom is available on the host.
pub async fn engine_status() -> Json<EngineReport> {
    Json(engine_report())
}
