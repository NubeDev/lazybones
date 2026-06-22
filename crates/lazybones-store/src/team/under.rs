//! Containment edges into the org (`team ->under-> org`) and the downward
//! traversal that lists a container's children.
//!
//! `under` is the authz/visibility spine (projects.md "containment vs assignment";
//! decisions §2): a child points `under` exactly one parent (`under_in_unique`),
//! and a container reads its children through the `under_out` index. Same idiom as
//! [`depends_on`](crate::task): a real graph `RELATION`, written with `RELATE` and
//! guarded by `IF NOT EXISTS` so re-relating is a clean no-op.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::RecordId;

use crate::error::{Result, StoreError};

use super::model::Team;
use super::row::{TEAM_TABLE, TeamRow};

/// The org-graph containment relation table.
pub(crate) const UNDER_TABLE: &str = "under";

/// Relate `team ->under-> org`, placing the team in the org (idempotent).
///
/// A child has exactly one parent (`under_in_unique`); the deterministic edge id
/// keeps re-relating the *same* pair a no-op.
///
/// # Errors
/// Returns [`StoreError::OrgNotFound`] if no such org exists, or
/// [`StoreError::Operation`] if the write fails.
pub async fn place_team_under_org(db: &Surreal<Db>, team: &str, org: &str) -> Result<()> {
    let parent: Option<crate::org::Org> = crate::org::get_org(db, org).await?;
    if parent.is_none() {
        return Err(StoreError::OrgNotFound(org.to_owned()));
    }
    relate_under(db, TEAM_TABLE, team, "org", org).await
}

/// The teams placed directly `under` `org`, via the `under_out` index.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn org_teams(db: &Surreal<Db>, org: &str) -> Result<Vec<Team>> {
    let parent = RecordId::new("org", org);
    let rows: Vec<TeamRow> = db
        .query(format!(
            "SELECT VALUE in.* FROM {UNDER_TABLE} \
             WHERE out = $parent AND meta::tb(in) = '{TEAM_TABLE}'"
        ))
        .bind(("parent", parent))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(TeamRow::into_team).collect())
}

/// Relate `child ->under-> parent`, deterministic-id and idempotent — the shared
/// write behind every containment placement (team→org, project→team).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub(crate) async fn relate_under(
    db: &Surreal<Db>,
    child_table: &str,
    child_id: &str,
    parent_table: &str,
    parent_id: &str,
) -> Result<()> {
    let from = RecordId::new(child_table, child_id);
    let to = RecordId::new(parent_table, parent_id);
    let edge = RecordId::new(UNDER_TABLE, format!("{child_id}__{parent_id}"));
    db.query(
        "IF !(SELECT id FROM ONLY $edge) { RELATE $from->under->$to SET id = $edge }",
    )
    .bind(("edge", edge))
    .bind(("from", from))
    .bind(("to", to))
    .await
    .map_err(StoreError::Operation)?
    .check()
    .map_err(StoreError::Operation)?;
    Ok(())
}
