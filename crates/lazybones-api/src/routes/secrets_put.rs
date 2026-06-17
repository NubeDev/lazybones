//! `PUT /secrets/:tool` — store (or rotate) an agent CLI credential.
//!
//! The value is sealed with the run's master key before it touches the DB. The
//! response is metadata only (no plaintext echo). Loop-guarded: managing
//! credentials is an operator action, never an agent one.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::SecretMeta;

use crate::dto::SecretBody;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Seal and store the credential for `tool`. Requires `Secret`.
pub async fn put_secret(
    State(state): State<AppState>,
    session: Session,
    Path(tool): Path<String>,
    Json(body): Json<SecretBody>,
) -> ApiResult<Json<SecretMeta>> {
    session.require(Capability::Secret, "secrets:put", "")?;
    let meta = state
        .store
        .put_secret(&tool, &body.env_var, &body.value)
        .await?;
    Ok(Json(meta))
}
