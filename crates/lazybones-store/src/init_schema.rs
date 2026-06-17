//! Idempotent schema for a lazybones run.
//!
//! The DB is the single source of truth (SCOPE.md principle 6). Three planes:
//! `task` documents, the `depends_on` graph relation that drives readiness, and
//! `event` run-log rows (one per status transition). All `SCHEMALESS` so the
//! record shape stays owned by the row types in Rust; only the columns a query
//! lives or dies on are declared. Memory (vectors) lands behind the same seam
//! later — the table is declared so a recall against an empty run returns no rows
//! rather than erroring.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

/// The table definitions every run namespace needs before first read.
const SCHEMA: &str = "\
DEFINE TABLE IF NOT EXISTS task SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS status ON task TYPE string;\n\
DEFINE FIELD IF NOT EXISTS run ON task TYPE string;\n\
DEFINE INDEX IF NOT EXISTS task_run_status ON task FIELDS run, status;\n\
DEFINE TABLE IF NOT EXISTS depends_on TYPE RELATION SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS event SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS task ON event TYPE string;\n\
DEFINE FIELD IF NOT EXISTS at ON event TYPE datetime;\n\
DEFINE INDEX IF NOT EXISTS event_task_at ON event FIELDS task, at;\n\
DEFINE TABLE IF NOT EXISTS memory SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS learned TYPE RELATION SCHEMALESS;";

/// Apply the schema on the bootstrapped connection.
///
/// Idempotent via `IF NOT EXISTS`.
///
/// # Errors
/// Returns [`StoreError::Bootstrap`] if a schema statement fails to apply.
pub async fn init_schema(db: &Surreal<Db>) -> Result<()> {
    db.query(SCHEMA)
        .await
        .map_err(StoreError::Bootstrap)?
        .check()
        .map_err(StoreError::Bootstrap)?;
    Ok(())
}
