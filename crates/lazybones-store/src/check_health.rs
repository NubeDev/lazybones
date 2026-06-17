//! Health probe against an open engine.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

/// Probe the engine by executing a trivial query.
///
/// # Errors
/// Returns [`StoreError::Health`] if the engine does not answer the probe.
pub async fn probe(db: &Surreal<Db>) -> Result<()> {
    db.query("RETURN true").await.map_err(StoreError::Health)?;
    Ok(())
}
