//! Containment of a project into its team (`project ->under-> team`) and the
//! downward traversal that lists a team's projects.
//!
//! Reuses the shared [`relate_under`](crate::team::relate_under) write and the
//! `under_out` index, the same authz spine teams use into the org. The owning team
//! is also denormalized onto the project row (see [`update_project`](super::update_project))
//! so the common "projects in my team" read is a single indexed scan
//! ([`list_projects`](super::list_projects)); this `under` edge remains the
//! source of truth.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::RecordId;

use crate::error::{Result, StoreError};

use super::model::Project;
use super::row::{PROJECT_TABLE, ProjectRow};

/// Relate `project ->under-> team`, placing the project in the team (idempotent).
///
/// # Errors
/// Returns [`StoreError::TeamNotFound`] if no such team exists, or
/// [`StoreError::Operation`] if the write fails.
pub async fn place_project_under_team(db: &Surreal<Db>, project: &str, team: &str) -> Result<()> {
    if crate::team::get_team(db, team).await?.is_none() {
        return Err(StoreError::TeamNotFound(team.to_owned()));
    }
    crate::team::relate_under(db, PROJECT_TABLE, project, "team", team).await
}

/// The projects placed directly `under` `team`, via the `under_out` index.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn team_projects(db: &Surreal<Db>, team: &str) -> Result<Vec<Project>> {
    let parent = RecordId::new("team", team);
    let rows: Vec<ProjectRow> = db
        .query(format!(
            "SELECT VALUE in.* FROM under \
             WHERE out = $parent AND meta::tb(in) = '{PROJECT_TABLE}'"
        ))
        .bind(("parent", parent))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(ProjectRow::into_project).collect())
}
