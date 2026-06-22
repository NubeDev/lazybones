//! `/teams/:id/members` — team membership (`user ->member_of-> team`) carrying the
//! per-team role.
//!
//! Add (with a `manager`/`member` role) and remove. Managing membership and roles
//! is an administrative act (projects.md Roles: "Admin … manage … membership/
//! roles"), so both verbs require [`Capability::Author`] *and* clear the role
//! guard as an admin. In local single-operator mode the role guard no-ops. The
//! per-team `manager`/`member` distinction lives on this edge; the global `admin`
//! flag is on the user.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::Membership;

use crate::dto::AddMemberBody;
use crate::error::ApiResult;
use crate::extract::Session;
use super::guard::{RoleReq, ensure_role};
use crate::state::AppState;

/// `GET /teams/:id/members` — the team's members with their per-team roles (open
/// read).
pub async fn list_members(
    State(state): State<AppState>,
    Path(team): Path<String>,
) -> ApiResult<Json<Vec<Membership>>> {
    Ok(Json(state.store.members_of(&team).await?))
}

/// `POST /teams/:id/members` — add (or re-affirm) a member with a role. Requires
/// `Author` + admin. Idempotent on the `(user, team)` pair; `404` if the team is
/// unknown.
pub async fn add_member(
    State(state): State<AppState>,
    session: Session,
    Path(team): Path<String>,
    Json(body): Json<AddMemberBody>,
) -> ApiResult<Json<Membership>> {
    session.require(Capability::Author, "author", &team)?;
    ensure_role(&state, &session, RoleReq::Admin).await?;

    state
        .store
        .add_member(&body.user, &team, body.role.into())
        .await?;
    Ok(Json(Membership {
        user: body.user,
        role: body.role.into(),
    }))
}

/// `DELETE /teams/:id/members/:user` — remove a membership. Requires `Author` +
/// admin. Returns whether a membership existed (idempotent).
pub async fn remove_member(
    State(state): State<AppState>,
    session: Session,
    Path((team, user)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    session.require(Capability::Author, "author", &team)?;
    ensure_role(&state, &session, RoleReq::Admin).await?;

    let removed = state.store.remove_member(&user, &team).await?;
    Ok(Json(serde_json::json!({ "removed": removed })))
}
