//! List projects (`GET /projects`), optionally narrowed by team.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Project;
use super::row::{PROJECT_TABLE, ProjectRow};

/// List every project, optionally narrowed to one owning `team` via the
/// denormalized `team` column (the indexed "projects in my team" read). Passing
/// `None` lists across all teams.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_projects(db: &Surreal<Db>, team: Option<&str>) -> Result<Vec<Project>> {
    let mut sql = format!("SELECT * FROM {PROJECT_TABLE}");
    if team.is_some() {
        sql.push_str(" WHERE team = $team");
    }
    let rows: Vec<ProjectRow> = db
        .query(sql)
        .bind(("team", team.map(ToOwned::to_owned)))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(ProjectRow::into_project).collect())
}
