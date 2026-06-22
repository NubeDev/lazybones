//! Membership edges (`user ->member_of-> team`) carrying the per-team role.
//!
//! Roles split two ways (projects.md "Roles", decisions §2 notes): the global
//! `admin` authority is a flag on the [`user`](crate::user); the per-team
//! `manager`/`member` distinction rides on *this* edge, so the same person can
//! manage team A and merely belong to team B. Same `RELATE … IF NOT EXISTS` idiom
//! as [`under`](super::under); `member_of_unique` keeps one edge per `(user, team)`.

use serde::{Deserialize, Serialize};
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{RecordId, SurrealValue};

use crate::error::{Result, StoreError};

use super::row::{TEAM_TABLE, TeamRow};

/// The membership relation table.
pub(crate) const MEMBER_OF_TABLE: &str = "member_of";

/// A member's authority *within a team*. Global `admin` is a flag on the user, not
/// a role here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    /// Creates/archives projects, assigns workflows, sees all team status.
    Manager,
    /// Works what's assigned; drives their own tasks.
    Member,
}

impl MemberRole {
    /// The lowercase wire form stored on the edge.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MemberRole::Manager => "manager",
            MemberRole::Member => "member",
        }
    }

    /// Parse the stored form, defaulting an unknown/missing value to `Member`.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("manager") => MemberRole::Manager,
            _ => MemberRole::Member,
        }
    }
}

/// One team membership: who, and with what per-team role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    /// The member's user id.
    pub user: String,
    /// Their role on this team's edge.
    pub role: MemberRole,
}

/// Add (or re-affirm) `user ->member_of-> team` with `role` (idempotent on the
/// `(user, team)` pair). Re-adding the same pair is a no-op — it does **not** flip
/// an already-stored role.
///
/// # Errors
/// Returns [`StoreError::TeamNotFound`] if no such team exists, or
/// [`StoreError::Operation`] if the write fails.
pub async fn add_member(
    db: &Surreal<Db>,
    user: &str,
    team: &str,
    role: MemberRole,
) -> Result<()> {
    let exists: Option<TeamRow> = db
        .select((TEAM_TABLE, team.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    if exists.is_none() {
        return Err(StoreError::TeamNotFound(team.to_owned()));
    }

    let from = RecordId::new("user", user);
    let to = RecordId::new(TEAM_TABLE, team);
    let edge = RecordId::new(MEMBER_OF_TABLE, format!("{user}__{team}"));
    db.query(
        "IF !(SELECT id FROM ONLY $edge) \
         { RELATE $from->member_of->$to SET id = $edge, role = $role }",
    )
    .bind(("edge", edge))
    .bind(("from", from))
    .bind(("to", to))
    .bind(("role", role.as_str().to_owned()))
    .await
    .map_err(StoreError::Operation)?
    .check()
    .map_err(StoreError::Operation)?;
    Ok(())
}

/// A row of the membership read: the member's plain user id and stored role.
#[derive(Debug, Clone, SurrealValue)]
struct MemberQueryRow {
    user_id: String,
    role: Option<String>,
}

/// The members of `team`, each with their per-team role (via `member_of`).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn members_of(db: &Surreal<Db>, team: &str) -> Result<Vec<Membership>> {
    let to = RecordId::new(TEAM_TABLE, team);
    let rows: Vec<MemberQueryRow> = db
        .query(format!(
            "SELECT meta::id(in) AS user_id, role FROM {MEMBER_OF_TABLE} WHERE out = $team"
        ))
        .bind(("team", to))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows
        .into_iter()
        .map(|r| Membership {
            user: r.user_id,
            role: MemberRole::parse(r.role.as_deref()),
        })
        .collect())
}
