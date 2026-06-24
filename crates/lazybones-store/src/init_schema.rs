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
DEFINE INDEX IF NOT EXISTS task_run_id ON task FIELDS run_id;\n\
DEFINE TABLE IF NOT EXISTS template SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS skill SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS attachment SCHEMALESS;\n\
DEFINE INDEX IF NOT EXISTS attachment_unique ON attachment FIELDS owner_kind, owner_id, thing_kind, thing_id UNIQUE;\n\
DEFINE INDEX IF NOT EXISTS attachment_owner ON attachment FIELDS owner_kind, owner_id;\n\
DEFINE TABLE IF NOT EXISTS agent SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS run SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS depends_on TYPE RELATION SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS event SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS task ON event TYPE string;\n\
DEFINE FIELD IF NOT EXISTS at ON event TYPE datetime;\n\
DEFINE INDEX IF NOT EXISTS event_task_at ON event FIELDS task, at;\n\
DEFINE TABLE IF NOT EXISTS hcom_log SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS run ON hcom_log TYPE string;\n\
DEFINE FIELD IF NOT EXISTS hcom_id ON hcom_log TYPE int;\n\
DEFINE INDEX IF NOT EXISTS hcom_log_run_id ON hcom_log FIELDS run, hcom_id UNIQUE;\n\
DEFINE INDEX IF NOT EXISTS hcom_log_run_task ON hcom_log FIELDS run, task;\n\
DEFINE TABLE IF NOT EXISTS chat SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS task ON chat TYPE string;\n\
DEFINE FIELD IF NOT EXISTS at ON chat TYPE datetime;\n\
DEFINE INDEX IF NOT EXISTS chat_task_at ON chat FIELDS task, at;\n\
DEFINE TABLE IF NOT EXISTS settings SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS agent_conversation SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS created_at ON agent_conversation TYPE option<string>;\n\
DEFINE TABLE IF NOT EXISTS agent_message SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS conversation_id ON agent_message TYPE string;\n\
DEFINE FIELD IF NOT EXISTS at ON agent_message TYPE datetime;\n\
DEFINE INDEX IF NOT EXISTS agent_message_conv_at ON agent_message FIELDS conversation_id, at;\n\
DEFINE TABLE IF NOT EXISTS memory SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS learned TYPE RELATION SCHEMALESS;\n\
DEFINE TABLE IF NOT EXISTS secret SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS env_var ON secret TYPE string;\n\
DEFINE TABLE IF NOT EXISTS follow_up SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS run ON follow_up TYPE string;\n\
DEFINE FIELD IF NOT EXISTS status ON follow_up TYPE string;\n\
DEFINE INDEX IF NOT EXISTS follow_up_run_dedup ON follow_up FIELDS run, dedup_key UNIQUE;\n\
DEFINE INDEX IF NOT EXISTS follow_up_run_status ON follow_up FIELDS run, status;\n\
DEFINE TABLE IF NOT EXISTS document SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS project ON document TYPE option<string>;\n\
DEFINE INDEX IF NOT EXISTS document_project ON document FIELDS project;\n\
DEFINE TABLE IF NOT EXISTS asset SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS project ON asset TYPE option<string>;\n\
DEFINE INDEX IF NOT EXISTS asset_project ON asset FIELDS project;\n\
DEFINE INDEX IF NOT EXISTS asset_sha256 ON asset FIELDS sha256;\n\
DEFINE TABLE IF NOT EXISTS extension SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS enabled ON extension TYPE option<bool>;\n\
DEFINE INDEX IF NOT EXISTS extension_enabled ON extension FIELDS enabled;\n\
DEFINE INDEX IF NOT EXISTS extension_sha256 ON extension FIELDS wasm_sha256;\n\
DEFINE TABLE IF NOT EXISTS branding SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS project ON branding TYPE option<string>;\n\
DEFINE INDEX IF NOT EXISTS branding_project ON branding FIELDS project;\n\
DEFINE TABLE IF NOT EXISTS source SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS project ON source TYPE option<string>;\n\
DEFINE FIELD IF NOT EXISTS document ON source TYPE string;\n\
DEFINE INDEX IF NOT EXISTS source_project ON source FIELDS project;\n\
DEFINE INDEX IF NOT EXISTS source_document ON source FIELDS document;\n\
DEFINE TABLE IF NOT EXISTS source_chunk SCHEMALESS;\n\
DEFINE FIELD IF NOT EXISTS source ON source_chunk TYPE option<string>;\n\
DEFINE FIELD IF NOT EXISTS vector ON source_chunk TYPE option<array<float>>;\n\
DEFINE INDEX IF NOT EXISTS source_chunk_vector ON source_chunk FIELDS vector HNSW DIMENSION 1536 DIST COSINE;";

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
