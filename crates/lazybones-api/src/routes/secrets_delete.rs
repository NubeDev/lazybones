//! `DELETE /secrets/:tool` — remove a stored credential.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use lazybones_auth::Capability;
use serde_json::{Value, json};

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Delete the credential for `tool`. `404` if none was stored. Requires `Secret`.
pub async fn delete_secret(
    State(state): State<AppState>,
    session: Session,
    Path(tool): Path<String>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    session.require(Capability::Secret, "secrets:delete", "")?;
    let existed = state.store.delete_secret(&tool).await?;
    let status = if existed {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    Ok((status, Json(json!({ "tool": tool, "deleted": existed }))))
}
