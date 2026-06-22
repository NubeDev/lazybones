//! CRUD for the agent catalog (`/agent-catalog`).
//!
//! The catalog defines each runnable agent CLI plus the models and effort levels
//! it offers — the data the add-task UI reads to populate its agent / model /
//! effort pickers. Reads are open (like `/templates`); mutations require
//! `Author` (loop-only), since they change install-wide config.
//!
//! Distinct from `GET /agents`, which reports *live* CLI availability + stored
//! credentials. This surface is the editable definition; that one is detection.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{AgentCatalog, AgentCatalogEdit, StoreError};

use crate::dto::{CreateAgentBody, UpdateAgentBody};
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// `GET /agent-catalog` — list every agent catalog entry (open read).
pub async fn list_agents(State(state): State<AppState>) -> ApiResult<Json<Vec<AgentCatalog>>> {
    Ok(Json(state.store.list_agents().await?))
}

/// `GET /agent-catalog/:id` — fetch one entry, or `404`.
pub async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<AgentCatalog>> {
    let agent = state
        .store
        .get_agent(&id)
        .await?
        .ok_or(StoreError::AgentNotFound(id))?;
    Ok(Json(agent))
}

/// `POST /agent-catalog` — author a new entry. Requires `Author`. `409` if the
/// id is already taken.
pub async fn create_agent(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateAgentBody>,
) -> ApiResult<Json<AgentCatalog>> {
    session.require(Capability::Author, "author", &body.id)?;
    let now = state.store.now();
    let agent = AgentCatalog::new(
        &body.id,
        &body.label,
        &body.env_var,
        &body.login_hint,
        body.models.clone(),
        body.default_model.clone(),
        body.efforts.clone(),
        body.default_effort.clone(),
        now,
    );
    Ok(Json(state.store.create_agent(&agent).await?))
}

/// `PATCH /agent-catalog/:id` — overwrite the authored fields. Requires `Author`.
/// `404` if no such entry exists.
pub async fn update_agent(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<UpdateAgentBody>,
) -> ApiResult<Json<AgentCatalog>> {
    session.require(Capability::Author, "author", &id)?;
    let edit = AgentCatalogEdit {
        label: body.label,
        env_var: body.env_var,
        login_hint: body.login_hint,
        models: body.models,
        default_model: body.default_model,
        efforts: body.efforts,
        default_effort: body.default_effort,
    };
    let now = state.store.now();
    Ok(Json(state.store.update_agent(&id, edit, &now).await?))
}

/// `DELETE /agent-catalog/:id` — remove an entry. Requires `Author`. Returns
/// whether it existed.
pub async fn delete_agent(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Author, "author", &id)?;
    let existed = state.store.delete_agent(&id).await?;
    Ok(Json(serde_json::json!({ "deleted": existed })))
}
