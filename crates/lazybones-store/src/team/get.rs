//! Read a single team by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Team;
use super::row::{TEAM_TABLE, TeamRow};

/// Read `team:<id>`, or `None` if no such team exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_team(db: &Surreal<Db>, id: &str) -> Result<Option<Team>> {
    let row: Option<TeamRow> = db
        .select((TEAM_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(TeamRow::into_team))
}
