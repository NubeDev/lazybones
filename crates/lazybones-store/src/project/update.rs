//! Edit an existing project document, preserving `created_at`.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Project;
use super::row::{PROJECT_TABLE, ProjectRow};

/// Overwrite the editable fields of `project:<id>` (title, status, team, repos),
/// failing if no such project exists. The original `created_at` is preserved;
/// `project.updated_at` is stored as the new update stamp.
///
/// Returns the stored project as it is after the write.
///
/// # Errors
/// Returns [`StoreError::ProjectNotFound`] if no project with that id exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_project(db: &Surreal<Db>, project: &Project) -> Result<Project> {
    let existing: Option<ProjectRow> = db
        .select((PROJECT_TABLE, project.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    let Some(existing) = existing else {
        return Err(StoreError::ProjectNotFound(project.id.clone()));
    };

    // Preserve the immutable creation stamp regardless of what the caller sent.
    let mut row = ProjectRow::from_project(project);
    row.created_at = existing.created_at;

    let written: Option<ProjectRow> = db
        .update((PROJECT_TABLE, project.id.as_str()))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(ProjectRow::into_project)
        .ok_or_else(|| StoreError::ProjectNotFound(project.id.clone()))
}
