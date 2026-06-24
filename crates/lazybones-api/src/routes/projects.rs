//! `/projects` — the team graph's ownership/authz root (projects.md "Project =
//! ownership root").
//!
//! CRUD plus **archive** (the project counterpart to the workflow
//! [`stop`](super::workflows_stop) idiom): archiving flips `status` to `archived`
//! and keeps the row for history rather than hard-deleting. Reads are open;
//! mutations require [`Capability::Author`] *and* clear the org-graph role guard
//! ([`ensure_role`](super::guard::ensure_role)) — a manager of the owning team, or
//! a global admin. In local single-operator mode the role guard no-ops.

use axum::Json;
use axum::extract::{Path, Query, State};
use lazybones_auth::Capability;
use lazybones_store::{Project, ProjectStatus, StoreError};

use crate::dto::{CreateProjectBody, TeamQuery, UpdateProjectBody};
use crate::error::ApiResult;
use crate::extract::Session;
use super::guard::{RoleReq, ensure_role};
use crate::state::AppState;

/// 404 unless the project exists.
async fn require_project(state: &AppState, id: &str) -> ApiResult<Project> {
    Ok(state
        .store
        .get_project(id)
        .await?
        .ok_or_else(|| StoreError::ProjectNotFound(id.to_owned()))?)
}

/// `GET /projects` — list projects (open read), optionally `?team=`.
pub async fn list_projects(
    State(state): State<AppState>,
    Query(query): Query<TeamQuery>,
) -> ApiResult<Json<Vec<Project>>> {
    Ok(Json(state.store.list_projects(query.team.as_deref()).await?))
}

/// `POST /projects` — author a project. Requires `Author` + manager of the owning
/// team (or admin). `409` on a taken id; `404` if the named team is unknown.
pub async fn create_project(
    State(state): State<AppState>,
    session: Session,
    Json(body): Json<CreateProjectBody>,
) -> ApiResult<Json<Project>> {
    session.require(Capability::Author, "author", &body.id)?;
    ensure_role(
        &state,
        &session,
        RoleReq::TeamManager {
            team: body.team.clone(),
        },
    )
    .await?;

    let mut project = Project::new(&body.id, &body.title, state.store.now());
    project.team = body.team.clone();
    project.repos = body.repos;
    let created = state.store.create_project(&project).await?;

    // Write the authoritative `under` containment edge so the team traversal finds
    // it; the denormalized `team` column already rode in on the row above.
    if let Some(team) = body.team.as_deref() {
        state.store.place_project_under_team(&created.id, team).await?;
    }
    Ok(Json(created))
}

/// `GET /projects/:id` — fetch one project (open read), or `404`.
pub async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Project>> {
    Ok(Json(require_project(&state, &id).await?))
}

/// `PUT /projects/:id` — overwrite a project's authored fields (title, repos).
/// Requires `Author` + manager of the owning team (or admin). `status`,
/// `created_at` and the owning `team` are preserved.
pub async fn update_project(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<UpdateProjectBody>,
) -> ApiResult<Json<Project>> {
    session.require(Capability::Author, "author", &id)?;
    let existing = require_project(&state, &id).await?;
    ensure_role(
        &state,
        &session,
        RoleReq::TeamManager {
            team: existing.team.clone(),
        },
    )
    .await?;

    let mut project = Project::new(&id, &body.title, state.store.now());
    project.status = existing.status;
    project.team = existing.team;
    project.repos = body.repos;
    Ok(Json(state.store.update_project(&project).await?))
}

/// `POST /projects/:id/archive` — shelve a project (`status → archived`), keeping
/// the row for history rather than hard-deleting (the project counterpart to the
/// workflow stop/delete split). Requires `Author` + manager of the owning team
/// (or admin). Idempotent: archiving an already-archived project is a no-op.
pub async fn archive_project(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<Project>> {
    session.require(Capability::Author, "author", &id)?;
    let mut existing = require_project(&state, &id).await?;
    ensure_role(
        &state,
        &session,
        RoleReq::TeamManager {
            team: existing.team.clone(),
        },
    )
    .await?;

    existing.status = ProjectStatus::Archived;
    existing.updated_at = state.store.now();
    Ok(Json(state.store.update_project(&existing).await?))
}
