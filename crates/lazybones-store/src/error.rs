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

    /// A template id was referenced that does not exist in the store.
    #[error("template not found: {0}")]
    TemplateNotFound(String),

    /// A template was created with an id that is already taken.
    #[error("template already exists: {0}")]
    TemplateExists(String),

    /// A skill id was referenced that does not exist in the store.
    #[error("skill not found: {0}")]
    SkillNotFound(String),

    /// A skill was created with an id that is already taken.
    #[error("skill already exists: {0}")]
    SkillExists(String),

    /// The bundled demo skill catalogue (YAML) failed to parse on seed.
    #[error("skill catalogue seed failed: {0}")]
    SkillSeed(String),

    /// A run (workflow) id was referenced that does not exist in the store.
    #[error("run not found: {0}")]
    RunNotFound(String),

    /// A run (workflow) was created with an id that is already taken.
    #[error("run already exists: {0}")]
    RunExists(String),

    /// An agent catalog id was referenced that does not exist in the store.
    #[error("agent not found: {0}")]
    AgentNotFound(String),

    /// An agent catalog entry was created with an id that is already taken.
    #[error("agent already exists: {0}")]
    AgentExists(String),

    /// The bundled default agent catalog (YAML) failed to parse on seed.
    #[error("agent catalog seed failed: {0}")]
    AgentSeed(String),

    /// Sealing or opening a secret failed (bad master key, corrupt blob, …).
    #[error("secret error: {0}")]
    Secret(String),
}
