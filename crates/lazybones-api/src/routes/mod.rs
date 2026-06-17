//! HTTP route table: one file per route (SCOPE.md REST surface).
//!
//! Memory (`POST /memory`, `GET /memory/recall`) is intentionally not wired yet —
//! the embedding provider is an open question (SCOPE.md OQ7); the store declares
//! the `memory` table so it can land behind this same router without a migration.

mod activity;
mod agent_test;
mod agents;
mod block;
mod cancel;
mod claim;
mod create;
mod delete;
mod done;
mod engine;
mod gate;
mod get;
mod health;
mod heartbeat;
mod list;
mod promote;
mod ready;
mod runs;
mod secrets_delete;
mod secrets_env;
mod secrets_list;
mod secrets_put;
mod stream;
mod sync;
mod update;

use axum::Router;
use axum::routing::{get, post, put};

use crate::cors::cors_layer;
use crate::state::AppState;

/// Assemble the full route table over the shared [`AppState`].
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/workfile/sync", post(sync::sync_workfile))
        .route("/tasks", get(list::list_tasks).post(create::create_task))
        .route("/tasks/promote", post(promote::promote_ready))
        .route("/tasks/:id/ready", post(ready::ready_task))
        .route(
            "/tasks/:id",
            get(get::get_task)
                .patch(update::update_task)
                .delete(delete::delete_task),
        )
        .route("/tasks/:id/claim", post(claim::claim_task))
        .route("/tasks/:id/heartbeat", post(heartbeat::heartbeat))
        .route("/tasks/:id/activity", post(activity::report_activity))
        .route("/tasks/:id/gate", post(gate::gate_task))
        .route("/tasks/:id/done", post(done::done_task))
        .route("/tasks/:id/block", post(block::block_task))
        // Operator cancel: kill the live agent (hcom) then block the task.
        .route("/tasks/:id/cancel", post(cancel::cancel_task))
        .route("/runs/:id", get(runs::run_history))
        // Live push feed of status transitions (SSE) — for the dashboard + loop.
        .route("/stream", get(stream::stream))
        // Engine + agent availability (so the UI can show what's set up).
        .route("/engine", get(engine::engine_status))
        .route("/agents", get(agents::list_agents))
        // Live-test one agent's credential by launching it through hcom.
        .route("/agents/:tool/test", post(agent_test::test_agent_route))
        // The secret store: agent CLI credentials, encrypted at rest. The `env`
        // export (decrypts) and all mutations are loop-guarded by `Secret`.
        .route("/secrets", get(secrets_list::list_secrets))
        .route("/secrets/env", get(secrets_env::secret_env))
        .route(
            "/secrets/:tool",
            put(secrets_put::put_secret).delete(secrets_delete::delete_secret),
        )
        // Let the browser/desktop UI (a different origin) read the surface.
        .layer(cors_layer())
        .with_state(state)
}
