//! `GET /secrets/env` — decrypt every credential to `env_var → value` pairs.
//!
//! The one route that returns plaintext. The trusted loop calls it, exports the
//! pairs, and spawns each agent CLI with its key in the environment. Guarded by
//! `Secret` (loop only); an agent session can never reach it.

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;
use lazybones_store::SecretEnv;

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Decrypt all secrets into env pairs for the loop. Requires `Secret`.
pub async fn secret_env(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<Vec<SecretEnv>>> {
    session.require(Capability::Secret, "secrets:env", "")?;
    Ok(Json(state.store.secret_env().await?))
}
