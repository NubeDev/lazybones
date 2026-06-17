//! Open the embedded SurrealDB engine that backs a lazybones run.
//!
//! Same engine choice the rubix platform standardises on (embedded, file-backed
//! SurrealKV for a real run; in-memory for tests). The namespace/database are
//! selected in [`crate::bootstrap`], not here.

use surrealdb::Surreal;
use surrealdb::engine::local::{Db, Mem, SurrealKv};

use crate::error::{Result, StoreError};

/// Where the embedded engine keeps its data.
#[derive(Debug, Clone)]
pub enum StoreEngine {
    /// In-memory, dropped on shutdown — used by tests.
    Memory,
    /// File-backed SurrealKV at `path` — used by a running `lazybonesd`.
    File {
        /// Directory the SurrealKV files live in.
        path: String,
    },
}

/// Open the engine described by `engine`, returning the raw SurrealDB client.
///
/// # Errors
/// Returns [`StoreError::Connect`] if the engine cannot be opened.
pub async fn open_engine(engine: &StoreEngine) -> Result<Surreal<Db>> {
    match engine {
        StoreEngine::Memory => Surreal::new::<Mem>(()).await.map_err(StoreError::Connect),
        StoreEngine::File { path } => Surreal::new::<SurrealKv>(path.as_str())
            .await
            .map_err(StoreError::Connect),
    }
}
