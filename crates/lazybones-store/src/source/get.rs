//! Read a single source by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::Source;
use super::row::{SOURCE_TABLE, SourceRow};

/// Read `source:<id>`, or `None` if no such source exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_source(db: &Surreal<Db>, id: &str) -> Result<Option<Source>> {
    let row: Option<SourceRow> = db
        .select((SOURCE_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(SourceRow::into_source))
}
