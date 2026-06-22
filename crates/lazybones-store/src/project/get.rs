//! Read a single project by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Project;
use super::row::{PROJECT_TABLE, ProjectRow};

/// Read `project:<id>`, or `None` if no such project exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_project(db: &Surreal<Db>, id: &str) -> Result<Option<Project>> {
    let row: Option<ProjectRow> = db
        .select((PROJECT_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(ProjectRow::into_project))
}
