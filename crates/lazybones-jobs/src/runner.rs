//! The [`JobRunner`] — runs registered jobs synchronously or asynchronously.

use std::sync::Arc;

use tokio::task::JoinHandle;

use crate::error::JobError;
use crate::job::{Job, JobReport};
use crate::registry::JobRegistry;

/// Runs jobs from a shared [`JobRegistry`], in two modes:
///
/// - **synchronous** — [`run_now`](Self::run_now) awaits the job to completion and
///   hands back its [`JobReport`]. Use when the caller wants the result (an API
///   request: "sync now and tell me what happened").
/// - **asynchronous** — [`spawn`](Self::spawn) launches the job on a Tokio task
///   and returns a [`JoinHandle`] immediately. Use for fire-and-forget background
///   work (a boot-time pull, a periodic push) where the caller shouldn't block.
///
/// Both paths run the *same* `Job`, so a job is written once and the call site
/// chooses the mode. The runner reuses Tokio's scheduler rather than owning a
/// thread pool of its own — it's a thin policy layer, not a new executor.
#[derive(Clone, Default)]
pub struct JobRunner {
    registry: JobRegistry,
}

impl JobRunner {
    /// Build a runner over `registry`.
    #[must_use]
    pub fn new(registry: JobRegistry) -> Self {
        Self { registry }
    }

    /// The registry this runner resolves names against.
    #[must_use]
    pub fn registry(&self) -> &JobRegistry {
        &self.registry
    }

    /// Run the job named `name` to completion and return its report
    /// (synchronous).
    ///
    /// # Errors
    /// Returns [`JobError::Unknown`] if no such job is registered, or whatever the
    /// job's `run` returns on failure.
    pub async fn run_now(&self, name: &str) -> Result<JobReport, JobError> {
        let job = self.registry.get(name)?;
        run_logged(job).await
    }

    /// Spawn the job named `name` on a Tokio task and return a handle to its
    /// result (asynchronous). The job runs even if the handle is dropped; await
    /// the handle only if you want the outcome.
    ///
    /// Resolution happens *before* spawning, so an unknown name fails fast here
    /// rather than inside the task.
    ///
    /// # Errors
    /// Returns [`JobError::Unknown`] if no such job is registered.
    pub fn spawn(&self, name: &str) -> Result<JobHandle, JobError> {
        let job = self.registry.get(name)?;
        let handle = tokio::spawn(async move { run_logged(job).await });
        Ok(JobHandle {
            name: name.to_owned(),
            handle,
        })
    }
}

/// A handle to a spawned job. Awaiting it yields the job's result, flattening the
/// Tokio [`JoinError`](tokio::task::JoinError) (panic/cancel) into a
/// [`JobError::Join`] so the caller only deals with one error type.
#[derive(Debug)]
pub struct JobHandle {
    name: String,
    handle: JoinHandle<Result<JobReport, JobError>>,
}

impl JobHandle {
    /// The name of the job this handle tracks.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Abort the spawned job. A subsequent [`join`](Self::join) reports
    /// [`JobError::Join`].
    pub fn abort(&self) {
        self.handle.abort();
    }

    /// Wait for the job to finish and return its result.
    ///
    /// # Errors
    /// Returns [`JobError::Join`] if the task panicked or was aborted, else the
    /// job's own `run` result.
    pub async fn join(self) -> Result<JobReport, JobError> {
        match self.handle.await {
            Ok(result) => result,
            Err(e) => Err(JobError::Join {
                job: self.name,
                message: e.to_string(),
            }),
        }
    }
}

/// Run a resolved job with uniform tracing on both paths.
async fn run_logged(job: Arc<dyn Job>) -> Result<JobReport, JobError> {
    let name = job.name().to_owned();
    tracing::debug!(job = %name, "job starting");
    match job.run().await {
        Ok(report) => {
            tracing::info!(job = %name, summary = %report.summary, "job finished");
            Ok(report)
        }
        Err(e) => {
            tracing::warn!(job = %name, error = %e, "job failed");
            Err(e)
        }
    }
}
