//! `/teams` — the mid container of the team graph (a team owns projects, has
//! members).
//!
//! List/get/create plus the downward traversal that lists a team's projects
//! (`project ->under-> team`). Reads are open; creating a team is an
//! administrative act — it requires [`Capability::Author`] *and* clears the
//! org-graph role guard as an admin ([`RoleReq::Admin`]). In local
//! single-operator mode the role guard no-ops.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Project, StoreError, Team};

use crate::dto::CreateTeamBody;
use crate::error::ApiResult;
use crate::extract::Session;
use super::guard::{RoleReq, ensure_role};
use crate::state::AppState;

/// `GET /teams` — list every team (open read).
pub async fn list_teams(State(state): State<AppState>) -> ApiResult<Json<Vec<Team>>> {
    Ok(Json(state.store.list_teams().await?))
}

/// `POST /teams` — create (or re-affirm) a team. Requires `Author` + admin.
/// Idempotent on the id (the org graph is cloud-authored, single-writer).
pub async fn create_team(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateTeamBody>,
) -> ApiResult<Json<Team>> {
    session.require(Capability::Author, "author", &body.id)?;
    ensure_role(&state, &session, RoleReq::Admin).await?;

    let team = Team::new(&body.id, &body.title, state.store.now());
    Ok(Json(state.store.create_team(&team).await?))
}

/// `GET /teams/:id` — fetch one team (open read), or `404`.
pub async fn get_team(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Team>> {
    Ok(Json(
        state
            .store
            .get_team(&id)
            .await?
            .ok_or_else(|| StoreError::TeamNotFound(id.clone()))?,
    ))
}

/// `GET /teams/:id/projects` — the projects placed `under` this team, via the
/// containment traversal (open read). An unknown team simply lists empty.
pub async fn list_team_projects(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Project>>> {
    Ok(Json(state.store.team_projects(&id).await?))
}
