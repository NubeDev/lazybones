//! Shared route guard: a task-level revive verb may only act when the parent
//! workflow is in a revivable lifecycle.
//!
//! The task-level revive verbs (`retry`, `auto-retry`, chat-revive) operate
//! directly on a task. Without this guard a `stopped` (paused) workflow's tasks
//! stay revivable — and because resume flips the run back to `active`, a revived
//! task would be re-claimed the moment the run resumes, so "stopped" would lie:
//! the UI says paused while work quietly continues. This refuses those verbs with
//! `409` so the operator must [`resume`](super::workflows_resume) the workflow
//! first. Standalone tasks (no parent run) are always revivable.

use lazybones_auth::AuthError;
use lazybones_store::{Lifecycle, MemberRole, Task};

use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// Refuse (`409`) if `task`'s parent workflow is not in a revivable lifecycle
/// (i.e. it is `stopped`). A task with no parent run, or whose run row is missing,
/// is treated as revivable. Errors only on a store read failure.
pub async fn ensure_run_revivable(state: &AppState, task: &Task) -> ApiResult<()> {
    // Prefer the `run_id` FK, falling back to the `run` label. A standalone task
    // has no matching workflow row, so the `get_run` below returns `None` and it
    // is treated as revivable.
    let run_key = task.run_id.as_deref().unwrap_or(task.run.as_str());

    let Some(run) = state.store.get_run(run_key).await? else {
        // No workflow row — a standalone task; nothing to gate on.
        return Ok(());
    };

    if run.lifecycle != Lifecycle::Active {
        return Err(ApiError::conflict(format!(
            "workflow `{run_key}` is {}; resume it before reviving task `{}`",
            run.lifecycle.as_str(),
            task.id
        )));
    }
    Ok(())
}

/// The org-graph authority a mutating verb demands of the calling principal,
/// gated by [`ensure_role`] against the team graph (projects.md "Roles").
///
/// This is the D5 "option B" locus (review-resolutions.md): the daemon holds one
/// root SurrealDB session and authorization is enforced here, in guard clauses —
/// **not** by a per-request `db.authenticate(jwt)`, which would race on the shared
/// handle. The principal is the session's [`actor`](Session::actor); in roles mode
/// that string is the user id in the graph.
pub enum RoleReq {
    /// Org-wide authority — managing teams, users, membership and roles. Only a
    /// global `admin` clears this (projects.md Roles: "Admin … manage teams,
    /// users, roles").
    Admin,
    /// Authority over one team's projects — create/archive/edit and assign work,
    /// see status (projects.md Roles: "Team Manager"). A global `admin` or a
    /// `manager` of `team` clears it. A teamless project (`team == None`) has no
    /// managing team, so it collapses to [`Admin`](Self::Admin).
    TeamManager { team: Option<String> },
}

/// Refuse (`403`) unless the `session`'s principal holds the org-graph authority
/// `req` demands.
///
/// **Local single-operator mode** ([`AppState::roles_enabled`] is `false`, no
/// `[server]` config): there are no roles, so this is a pass-through — the daemon
/// trusts its one operator exactly as today. A global `admin` clears every verb;
/// otherwise the per-team `member_of` role is consulted.
///
/// # Errors
/// Returns [`ApiError::Forbidden`] if the principal lacks the required role, or a
/// store error if a graph read fails.
pub async fn ensure_role(state: &AppState, session: &Session, req: RoleReq) -> ApiResult<()> {
    // No `[server]` config ⇒ no roles: trust the single operator (no-op).
    if !state.roles_enabled() {
        return Ok(());
    }

    let principal = session.actor();
    // A global admin clears everything (the `admin` bool on the user row).
    if state
        .store
        .get_user(principal)
        .await?
        .is_some_and(|u| u.admin)
    {
        return Ok(());
    }

    match req {
        RoleReq::Admin => Err(forbidden("admin")),
        RoleReq::TeamManager { team: Some(team) } => {
            let is_manager = state
                .store
                .members_of(&team)
                .await?
                .into_iter()
                .any(|m| m.user == principal && m.role == MemberRole::Manager);
            if is_manager {
                Ok(())
            } else {
                Err(forbidden(&format!("manager of team `{team}`")))
            }
        }
        // A teamless project has no managing team — only an admin may touch it.
        RoleReq::TeamManager { team: None } => Err(forbidden("admin")),
    }
}

/// A `403` carrying which authority the verb required.
fn forbidden(required: &str) -> ApiError {
    ApiError::Forbidden(AuthError::ForbiddenRole(format!(
        "requires {required}"
    )))
}
