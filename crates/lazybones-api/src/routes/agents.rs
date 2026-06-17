//! `GET /agents` — which agent CLIs are installed and set up.
//!
//! Joins the static agent catalog against live PATH/version detection and the
//! stored-credential set, so the UI's credentials panel can show, per tool:
//! installed?, key stored?, ready to run? Loop-guarded — it reveals which keys
//! are stored (not their values), an operator view.

use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use lazybones_auth::Capability;

use crate::engine::{AgentReport, agent_reports};
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Report agent CLI availability + setup. Requires `Secret`.
pub async fn list_agents(
    State(state): State<AppState>,
    session: Session,
) -> ApiResult<Json<Vec<AgentReport>>> {
    session.require(Capability::Secret, "agents:list", "")?;
    let stored: HashSet<String> = state
        .store
        .list_secrets()
        .await?
        .into_iter()
        .map(|m| m.tool)
        .collect();
    Ok(Json(agent_reports(&stored)))
}
