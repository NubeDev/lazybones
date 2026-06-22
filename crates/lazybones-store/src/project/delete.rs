//! Delete a project by id (`DELETE /projects/:id`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::row::{PROJECT_TABLE, ProjectRow};

/// Delete `project:<id>`. Returns whether a project existed.
///
/// Note: this does **not** cascade to its `under` containment edge or the
/// workflows scoped beneath it — those carry no hard FK and read as "the container
/// no longer exists".
///
/// # Errors
/// Returns [`StoreError::Operation`] if the delete fails.
pub async fn delete_project(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let deleted: Option<ProjectRow> = db
        .delete((PROJECT_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(deleted.is_some())
}
