//! `GET /secrets` — list stored credentials as metadata (no plaintext values).
//!
//! Drives the UI's credentials panel: which tools have a key set, a `…last4`
//! hint, and when each was written. Loop-guarded — even the safe metadata is an
//! operator view, not something an agent session enumerates.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::SecretMeta;

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// List stored secrets as safe metadata. Requires `Secret`.
pub async fn list_secrets(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<Vec<SecretMeta>>> {
    session.require(Capability::Secret, "secrets:list", "")?;
    Ok(Json(state.store.list_secrets().await?))
}
