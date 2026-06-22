//! Read a single run (workflow) by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Run;
use super::row::{RUN_TABLE, RunRow};

/// Read `run:<id>`, or `None` if no such run exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_run(db: &Surreal<Db>, id: &str) -> Result<Option<Run>> {
    let row: Option<RunRow> = db
        .select((RUN_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(RunRow::into_run))
}
