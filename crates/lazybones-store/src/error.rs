//! Store-domain errors for lazybones.

/// Convenience alias for store results.
pub type Result<T> = std::result::Result<T, StoreError>;

/// Failures raised by the embedded SurrealDB store boundary.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// Opening the embedded engine failed.
    #[error("failed to open store engine: {0}")]
    Connect(#[source] surrealdb::Error),

    /// Selecting or creating the namespace/database failed.
    #[error("failed to bootstrap namespace/database: {0}")]
    Bootstrap(#[source] surrealdb::Error),

    /// A health probe against the engine failed.
    #[error("store health probe failed: {0}")]
    Health(#[source] surrealdb::Error),

    /// A read or write through the durable boundary failed.
    #[error("store operation failed: {0}")]
    Operation(#[source] surrealdb::Error),

    /// A transition was requested that the task's current status forbids.
    #[error("illegal transition for task {task}: {from} -> {to}")]
    IllegalTransition {
        /// The task id the transition was requested for.
        task: String,
        /// The status the task is currently in.
        from: String,
        /// The status the transition tried to reach.
        to: String,
    },

    /// A task id was referenced that does not exist in the store.
    #[error("task not found: {0}")]
    TaskNotFound(String),

    /// A task was created with an id that is already taken.
    #[error("task already exists: {0}")]
    TaskExists(String),

    /// Sealing or opening a secret failed (bad master key, corrupt blob, …).
    #[error("secret error: {0}")]
    Secret(String),
}
