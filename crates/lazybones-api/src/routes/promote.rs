//! `POST /tasks/promote` — promote every pending task whose deps are now done.
//!
//! The loop calls this each pass: the store's graph readiness query finds the
//! `pending` tasks whose dependencies are all `done`, and each is transitioned
//! `pending → ready`. Returns the ids promoted, so the loop knows what to spawn.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::Transition;

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Promote all newly-ready tasks and return their ids. Requires `Claim`.
pub async fn promote_ready(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<Vec<String>>> {
    session.require(Capability::Claim, "promote", "")?;
    // A stopped (paused) workflow promotes nothing — exclude its tasks.
    let stopped = state.store.unpromotable_run_ids().await?;
    let ready = state.store.newly_ready(&stopped).await?;
    for id in &ready {
        state
            .store
            .transition(id, Transition::Ready, session.actor())
            .await?;
    }
    Ok(Json(ready))
}
