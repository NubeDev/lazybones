//! Idempotent creation of a team (cloud-authored, single-writer).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Team;
use super::row::{TEAM_TABLE, TeamRow};

/// Ensure `team:<id>` exists, returning it. The org graph is cloud-authored and
/// single-writer (decisions §3), so create is idempotent: re-creating an existing
/// id returns the stored record rather than erroring.
///
/// # Errors
/// Returns [`StoreError::TeamNotFound`] if the write reports no row, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn create_team(db: &Surreal<Db>, team: &Team) -> Result<Team> {
    let existing: Option<TeamRow> = db
        .select((TEAM_TABLE, team.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if let Some(row) = existing {
        return Ok(row.into_team());
    }

    let written: Option<TeamRow> = db
        .create((TEAM_TABLE, team.id.as_str()))
        .content(TeamRow::from_team(team))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(TeamRow::into_team)
        .ok_or_else(|| StoreError::TeamNotFound(team.id.clone()))
}
