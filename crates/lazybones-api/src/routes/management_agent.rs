//! `GET /settings/management-agent` + `PUT /settings/management-agent` — the
//! single global Lazybones-Agent configuration (`docs/agent/lazybones-agent-scope.md` §5).
//!
//! Reads return the stored config (or a usable default when unset). Writes
//! require `Author` and validate `tool`/`model`/`effort` against the agent
//! catalog, so the configured agent is always launchable.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{
    ManagementAgentConfig, ManagementAgentScope, PermissionProfile, SessionMode,
};

use crate::dto::ManagementAgentBody;
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// `GET /settings/management-agent` — the current config, or the default if the
/// operator has never saved one. Open read (local single-user daemon).
pub async fn get_management_agent(
    State(state): State<AppState>,
) -> ApiResult<Json<ManagementAgentConfig>> {
    let config = state
        .store
        .get_management_agent()
        .await?
        .unwrap_or_default();
    Ok(Json(config))
}

/// `PUT /settings/management-agent` — replace the global config. Requires
/// `Author`. `400` if the tool is unknown or model/effort is not in its catalog.
pub async fn put_management_agent(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<ManagementAgentBody>,
) -> ApiResult<Json<ManagementAgentConfig>> {
    session.require(Capability::Author, "author", "management-agent")?;
    let config = validate_and_build(&state, body).await?;
    Ok(Json(
        state
            .store
            .put_management_agent_scoped(&ManagementAgentScope::Global, &config)
            .await?,
    ))
}

/// `GET /settings/management-agent/workflows/:id` — the *resolved* config for a
/// workflow (its override if set, else the global default). Open read.
pub async fn get_workflow_management_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ManagementAgentConfig>> {
    let config = state
        .store
        .get_management_agent_resolved(&ManagementAgentScope::Workflow(id))
        .await?
        .unwrap_or_default();
    Ok(Json(config))
}

/// `PUT /settings/management-agent/workflows/:id` — set a per-workflow override.
/// Requires `Author`; same catalog validation as the global config.
pub async fn put_workflow_management_agent(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<ManagementAgentBody>,
) -> ApiResult<Json<ManagementAgentConfig>> {
    session.require(Capability::Author, "author", "management-agent")?;
    let config = validate_and_build(&state, body).await?;
    Ok(Json(
        state
            .store
            .put_management_agent_scoped(&ManagementAgentScope::Workflow(id), &config)
            .await?,
    ))
}

/// `DELETE /settings/management-agent/workflows/:id` — drop a workflow override,
/// reverting that workflow to the global default. Requires `Author`.
pub async fn delete_workflow_management_agent(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Author, "author", "management-agent")?;
    let existed = state
        .store
        .delete_management_agent_scoped(&ManagementAgentScope::Workflow(id))
        .await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}

/// Validate `body` against the agent catalog and build a [`ManagementAgentConfig`]
/// (shared by the global and per-workflow writers).
async fn validate_and_build(
    state: &AppState,
    body: ManagementAgentBody,
) -> ApiResult<ManagementAgentConfig> {
    let agent = state
        .store
        .get_agent(&body.tool)
        .await?
        .ok_or_else(|| ApiError::bad_request(format!("unknown tool `{}`", body.tool)))?;
    if let Some(model) = &body.model
        && !agent.models.contains(model)
    {
        return Err(ApiError::bad_request(format!(
            "model `{model}` is not offered by tool `{}`",
            body.tool
        )));
    }
    if let Some(effort) = &body.effort
        && !agent.efforts.contains(effort)
    {
        return Err(ApiError::bad_request(format!(
            "effort `{effort}` is not offered by tool `{}`",
            body.tool
        )));
    }

    Ok(ManagementAgentConfig {
        tool: body.tool,
        model: body.model,
        effort: body.effort,
        permission_profile: PermissionProfile::parse(&body.permission_profile),
        session_mode: SessionMode::parse(&body.session_mode),
        enabled_skills: body.enabled_skills,
        permission_flags: body.permission_flags,
        updated_at: state.store.now(),
    })
}
