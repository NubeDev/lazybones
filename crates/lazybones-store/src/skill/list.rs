//! List all skills (`GET /skills`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Skill;
use super::row::{SKILL_TABLE, SkillRow};

/// List every skill.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_skills(db: &Surreal<Db>) -> Result<Vec<Skill>> {
    let rows: Vec<SkillRow> = db
        .query(format!("SELECT * FROM {SKILL_TABLE}"))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(SkillRow::into_skill).collect())
}
