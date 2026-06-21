//! Strict creation of a skill document (authoring; must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Skill;
use super::row::{SKILL_TABLE, SkillRow};

/// Create `skill` as a new record, failing if its id is already in use.
///
/// Returns the stored skill as it is after the write.
///
/// # Errors
/// Returns [`StoreError::SkillExists`] if a skill with that id already exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn create_skill(db: &Surreal<Db>, skill: &Skill) -> Result<Skill> {
    let existing: Option<SkillRow> = db
        .select((SKILL_TABLE, skill.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::SkillExists(skill.id.clone()));
    }

    let written: Option<SkillRow> = db
        .create((SKILL_TABLE, skill.id.as_str()))
        .content(SkillRow::from_skill(skill))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(SkillRow::into_skill)
        .ok_or_else(|| StoreError::SkillNotFound(skill.id.clone()))
}
