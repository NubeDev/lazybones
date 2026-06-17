//! Delete a task document and tear down its dependency edges.
//!
//! A task never stands alone in the graph: other tasks may `depends_on` it, and
//! it may `depends_on` others. Deleting the record without dropping those edges
//! would leave a dependent task pointing at a ghost (the readiness traversal
//! would dangle), so both directions of the `depends_on` relation are cleared
//! first.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::RecordId;

use crate::error::{Result, StoreError};

use super::row::{TASK_TABLE, TaskRow};

/// Delete `task:<id>` along with every `depends_on` edge it sits on.
///
/// Edges where this task is the source (`in`) and where it is the target
/// (`out`) are both removed. Returns whether a task with that id existed.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the edge cleanup or the delete fails.
pub async fn delete_task(db: &Surreal<Db>, id: &str) -> Result<bool> {
    let thing = RecordId::new(TASK_TABLE, id);
    db.query("DELETE depends_on WHERE in = $task OR out = $task")
        .bind(("task", thing))
        .await
        .map_err(StoreError::Operation)?
        .check()
        .map_err(StoreError::Operation)?;

    let deleted: Option<TaskRow> = db
        .delete((TASK_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;

    Ok(deleted.is_some())
}
