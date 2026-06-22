//! Read a single skill by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Skill;
use super::row::{SKILL_TABLE, SkillRow};

/// Read `skill:<id>`, or `None` if no such skill exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_skill(db: &Surreal<Db>, id: &str) -> Result<Option<Skill>> {
    let row: Option<SkillRow> = db
        .select((SKILL_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(SkillRow::into_skill))
}
