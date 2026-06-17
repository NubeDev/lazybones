//! `GET /health` — liveness of the process and its store.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::state::AppState;

/// Probe the store and report liveness. `503` if the store does not answer.
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match state.store.health().await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "unavailable" })),
        ),
    }
}
