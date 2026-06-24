//! List all teams (`GET /teams`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Team;
use super::row::{TEAM_TABLE, TeamRow};

/// List every team.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_teams(db: &Surreal<Db>) -> Result<Vec<Team>> {
    let rows: Vec<TeamRow> = db
        .query(format!("SELECT * FROM {TEAM_TABLE}"))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(TeamRow::into_team).collect())
}
