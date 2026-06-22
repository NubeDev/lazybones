//! Edit an existing skill document, preserving `created_at`.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Skill;
use super::row::{SKILL_TABLE, SkillRow};

/// Overwrite the editable fields of `skill:<id>`, failing if no such skill
/// exists. The original `created_at` is preserved; `skill.updated_at` is stored
/// as the new update stamp.
///
/// Returns the stored skill as it is after the write.
///
/// # Errors
/// Returns [`StoreError::SkillNotFound`] if no skill with that id exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_skill(db: &Surreal<Db>, skill: &Skill) -> Result<Skill> {
    let existing: Option<SkillRow> = db
        .select((SKILL_TABLE, skill.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    let Some(existing) = existing else {
        return Err(StoreError::SkillNotFound(skill.id.clone()));
    };

    // Preserve the immutable creation stamp regardless of what the caller sent.
    let mut row = SkillRow::from_skill(skill);
    row.created_at = existing.created_at;

    let written: Option<SkillRow> = db
        .update((SKILL_TABLE, skill.id.as_str()))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(SkillRow::into_skill)
        .ok_or_else(|| StoreError::SkillNotFound(skill.id.clone()))
}
