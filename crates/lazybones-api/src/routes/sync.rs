//! `POST /workfile/sync` — idempotent import of seed tasks into the store.
//!
//! Accepts the parsed workfile as a JSON array of seed tasks (the CLI reads the
//! YAML, resolves spec paths, and posts this). Delegates to the store's shared
//! [`sync_seeds`] so the route and the CLI boot import follow one path. The DB is
//! authoritative after (SCOPE.md principle 6). Requires `Sync` (loop-only).

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::{SeedTask, sync_seeds};

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Upsert every seed task and wire its dependency edges. Returns the count.
pub async fn sync_workfile(
    State(state): State<AppState>,
    session: Session,
    Json(seeds): Json<Vec<SeedTask>>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Sync, "sync", "")?;
    let synced = sync_seeds(&state.store, &state.run, &seeds).await?;
    Ok(Json(serde_json::json!({ "synced": synced })))
}
