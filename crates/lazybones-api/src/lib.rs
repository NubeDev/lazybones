//! REST surface for lazybones — the axum routes over the durable store.
//!
//! Verb-per-file routes (SCOPE.md): health, workfile sync, task list/get, and the
//! lifecycle mutations (claim, heartbeat, gate, done, block, promote) plus run
//! history. Every mutating route is guarded by a scoped session resolved from the
//! request's bearer token.

mod dto;
mod error;
mod extract;
mod routes;
mod state;

pub use error::{ApiError, ApiResult};
pub use routes::router;
pub use state::AppState;
