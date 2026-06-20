//! `GET /tasks/:id/transcript` — on-demand passthrough to hcom's deep transcript.
//!
//! When the hcom log's event spine (message/status/life) isn't enough and you
//! want the full conversation (tool I/O, file edits, assistant prose), this shells
//! out to `hcom transcript <agent> --json --full` for the task's still-known agent
//! and streams the result back. It is **not** a stored artifact — it can be large
//! and hcom owns it; it only works while hcom still retains the agent
//! (docs/hcom-logs-scope.md, OQ4).

use axum::Json;
use axum::extract::{Path, State};
use lazybones_store::StoreError;
use serde_json::Value;
use tokio::process::Command;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `GET /tasks/:id/transcript` — the deep hcom transcript for the task's agent.
///
/// `404` if the task is unknown or was never claimed (no agent); `502` if hcom is
/// missing, the agent is past hcom's retention, or the CLI errors.
pub async fn task_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let task = state
        .store
        .get_task(&id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.clone()))?;
    let agent = task.session.ok_or_else(|| {
        ApiError::bad_request(format!("task `{id}` has no agent session (never claimed)"))
    })?;

    let bin = std::env::var("HCOM_BIN").unwrap_or_else(|_| "hcom".to_owned());
    let mut cmd = Command::new(&bin);
    cmd.arg("transcript").arg(&agent).arg("--json").arg("--full");
    if let Some(dir) = std::env::var_os("HCOM_DIR") {
        cmd.env("HCOM_DIR", dir);
    }

    let out = cmd.output().await.map_err(|e| {
        ApiError::Internal(format!("could not launch hcom transcript: {e}"))
    })?;
    if !out.status.success() {
        return Err(ApiError::bad_request(format!(
            "hcom transcript for agent `{agent}` failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let value: Value = serde_json::from_str(stdout.trim()).map_err(|e| {
        ApiError::Internal(format!("hcom transcript returned non-JSON: {e}"))
    })?;
    Ok(Json(value))
}
