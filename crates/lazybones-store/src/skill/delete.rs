//! Delete a skill by id (`DELETE /skills/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{SKILL_TABLE, SkillRow};

/// Delete `skill:<id>`. Returns whether a skill existed.
///
/// Note: this does **not** cascade to [`attachment`](crate::attachment) rows that
/// reference the skill — attachments are polymorphic and carry no hard FK, so a
/// dangling attachment is tolerated (and reads as "the thing no longer exists").
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_skill(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<SkillRow> = db
        .delete((SKILL_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
