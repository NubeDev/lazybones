//! Errors from registering and running jobs.

/// Failures raised by the job service.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum JobError {
    /// A job was requested by a name that isn't in the registry.
    #[error("no job registered under `{0}`")]
    Unknown(String),

    /// A job was registered under a name already taken (registration is strict so
    /// a typo can't silently shadow a real job).
    #[error("a job is already registered under `{0}`")]
    Duplicate(String),

    /// The job ran but failed. Carries the job name and the underlying message;
    /// the source error is type-erased so the trait stays `dyn`-compatible and any
    /// job (gh sync, db export, …) can surface its own error kind here.
    #[error("job `{job}` failed: {message}")]
    Failed {
        /// The job that failed.
        job: String,
        /// A human-readable description of the failure.
        message: String,
    },

    /// The async task carrying a spawned job panicked or was cancelled before it
    /// produced a result (`tokio::task::JoinError`).
    #[error("job `{job}` did not complete: {message}")]
    Join {
        /// The job whose task failed to join.
        job: String,
        /// The join failure (panic / cancellation).
        message: String,
    },
}

impl JobError {
    /// Build a [`JobError::Failed`] for `job` from any error. The convenience a
    /// job's `run` returns: `.map_err(|e| JobError::failed("github-sync", e))`.
    pub fn failed(job: impl Into<String>, source: impl std::fmt::Display) -> Self {
        Self::Failed {
            job: job.into(),
            message: source.to_string(),
        }
    }
}
