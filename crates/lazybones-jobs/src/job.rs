//! The [`Job`] trait — one unit of background work — and its result.

use async_trait::async_trait;
use serde::Serialize;

use crate::error::JobError;

/// What a finished job reports back: which job ran and a short human summary
/// (e.g. "pushed 6 records at a1b2c3d", or "clean — nothing to sync"). Kept
/// deliberately small so it can cross the async boundary cheaply and be logged or
/// shown in a UI without the runner knowing any job's internals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobReport {
    /// The name of the job that produced this.
    pub job: String,
    /// A one-line, operator-facing summary of what happened.
    pub summary: String,
}

impl JobReport {
    /// Build a report for `job` with `summary`.
    pub fn new(job: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            job: job.into(),
            summary: summary.into(),
        }
    }
}

/// A unit of background work the [`JobRunner`](crate::JobRunner) can execute
/// either synchronously (await the result) or asynchronously (spawn and walk
/// away). A job is a self-contained recipe — it owns whatever handles it needs
/// (a `StoreHandle`, a `SyncRepo`) and exposes only `name` + `run`, so the runner
/// stays generic over *what* the work is.
///
/// This is the seam the long-term design hangs off: a "github-sync" job
/// (export → commit → push / pull → import), a "gdrive-sync" job, or anything
/// else, all run through the same `run_now` / `spawn`. When durability is wanted
/// later, a store-backed queue can drain the very same `Job`s — the trait doesn't
/// assume in-process execution.
///
/// Implementations must be `Send + Sync + 'static` so a spawned job can move onto
/// a Tokio worker thread and outlive the call that started it.
#[async_trait]
pub trait Job: Send + Sync + 'static {
    /// The stable, unique name this job is registered and invoked under
    /// (e.g. `"github-sync"`). Used as the registry key and in logs/errors.
    fn name(&self) -> &str;

    /// Do the work. Returns a [`JobReport`] on success; on failure return a
    /// [`JobError::Failed`] (see [`JobError::failed`](crate::JobError::failed) for
    /// the common `map_err` form).
    ///
    /// # Errors
    /// Returns [`JobError`] if the work fails.
    async fn run(&self) -> Result<JobReport, JobError>;
}
