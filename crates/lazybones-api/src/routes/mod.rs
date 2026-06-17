//! HTTP route table: one file per route (SCOPE.md REST surface).
//!
//! Memory (`POST /memory`, `GET /memory/recall`) is intentionally not wired yet —
//! the embedding provider is an open question (SCOPE.md OQ7); the store declares
//! the `memory` table so it can land behind this same router without a migration.

mod block;
mod claim;
mod done;
mod gate;
mod get;
mod health;
mod heartbeat;
mod list;
mod promote;
mod runs;
mod sync;

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

/// Assemble the full route table over the shared [`AppState`].
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/workfile/sync", post(sync::sync_workfile))
        .route("/tasks", get(list::list_tasks))
        .route("/tasks/promote", post(promote::promote_ready))
        .route("/tasks/:id", get(get::get_task))
        .route("/tasks/:id/claim", post(claim::claim_task))
        .route("/tasks/:id/heartbeat", post(heartbeat::heartbeat))
        .route("/tasks/:id/gate", post(gate::gate_task))
        .route("/tasks/:id/done", post(done::done_task))
        .route("/tasks/:id/block", post(block::block_task))
        .route("/runs/:id", get(runs::run_history))
        .with_state(state)
}
