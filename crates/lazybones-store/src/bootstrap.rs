//! Select (and implicitly create) the namespace/database on an open engine.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

/// Point `db` at the given namespace and database.
///
/// # Errors
/// Returns [`StoreError::Bootstrap`] if the namespace/database cannot be used.
pub async fn use_namespace(db: &Surreal<Db>, namespace: &str, database: &str) -> Result<()> {
    db.use_ns(namespace.to_owned())
        .use_db(database.to_owned())
        .await
        .map_err(StoreError::Bootstrap)?;
    Ok(())
}
