//! `POST /agents/:tool/test` — live-test an agent CLI's credential.
//!
//! Launches the agent through hcom in print mode with its stored credential (or
//! whatever is already in the daemon's env) and reports whether it actually
//! authenticated and replied. This is the real thing, not a presence check: a
//! bad key, a missing/unrunnable binary, or a refusal all come back as `ok:false`
//! with a reason. Loop-guarded — it decrypts a secret to run the probe, an
//! operator action an agent session must never reach.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;

use crate::engine::{AgentTestResult, env_var_for, test_agent};
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// Probe `tool`'s credential by launching it. Requires `Secret`.
pub async fn test_agent_route(
    State(state): State<AppState>,
    session: Session,
    Path(tool): Path<String>,
) -> ApiResult<Json<AgentTestResult>> {
    session.require(Capability::Secret, "agents:test", &tool)?;

    // Reject an unknown tool up front so we never spawn for it.
    let env_var = env_var_for(&tool).ok_or(ApiError::NotFound)?;

    // Decrypt the stored key for this tool, if any. The loop's env list is keyed
    // by env var (not tool), so match on the tool's catalog var. Absent is fine —
    // the probe falls back to the daemon's environment (e.g. a `claude login`).
    let key = state
        .store
        .secret_env()
        .await?
        .into_iter()
        .find(|e| e.env_var == env_var)
        .map(|e| e.value);

    // The probe spawns a real agent process; run it off the async runtime so the
    // blocking wait doesn't starve other requests.
    let result = tokio::task::spawn_blocking(move || test_agent(&tool, key.as_deref()))
        .await
        .map_err(|e| ApiError::Internal(format!("probe task failed: {e}")))?
        .ok_or(ApiError::NotFound)?;

    Ok(Json(result))
}
