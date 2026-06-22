//! Set a run's lifecycle back to `active` (the human-set un-pause).
//!
//! The reverse of [`stop_run`](super::stop::stop_run): flips only the stored
//! `lifecycle` field back to `active` so the scheduler resumes promoting and
//! claiming this run's tasks from wherever they were left. Re-attaching any
//! blocked tasks to the pipeline is the API's job (the resume route); the store
//! just records that the run is live again.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::Result;

use super::model::{Lifecycle, Run};
use super::stop::set_lifecycle;

/// Mark `run:<id>` active again. Returns the updated run.
///
/// # Errors
/// Returns [`StoreError::RunNotFound`](crate::StoreError::RunNotFound) if the run
/// does not exist, or [`StoreError::Operation`](crate::StoreError::Operation) if
/// the write fails.
pub async fn resume_run(db: &Surreal<Db>, id: &str) -> Result<Run> {
    set_lifecycle(db, id, Lifecycle::Active).await
}
