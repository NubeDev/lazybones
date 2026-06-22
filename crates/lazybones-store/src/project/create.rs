//! Strict creation of a project (authoring; must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Project;
use super::row::{PROJECT_TABLE, ProjectRow};

/// Create `project` as a new record, failing if its id is already in use.
///
/// Returns the stored project as it is after the write. The authoritative
/// `project ->under-> team` edge is written separately via
/// [`place_project_under_team`](super::place_project_under_team).
///
/// # Errors
/// Returns [`StoreError::ProjectExists`] if a project with that id already exists,
/// or [`StoreError::Operation`] if the read or write fails.
pub async fn create_project(db: &Surreal<Db>, project: &Project) -> Result<Project> {
    let existing: Option<ProjectRow> = db
        .select((PROJECT_TABLE, project.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::ProjectExists(project.id.clone()));
    }

    let written: Option<ProjectRow> = db
        .create((PROJECT_TABLE, project.id.as_str()))
        .content(ProjectRow::from_project(project))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(ProjectRow::into_project)
        .ok_or_else(|| StoreError::ProjectNotFound(project.id.clone()))
}
