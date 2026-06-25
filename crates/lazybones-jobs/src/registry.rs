//! The [`JobRegistry`] — a name → [`Job`] map the runner resolves against.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::JobError;
use crate::job::Job;

/// A registry of jobs keyed by [`Job::name`]. Cheap to clone (the jobs sit behind
/// `Arc`), so the runner and any callers can share one. `BTreeMap` keeps
/// [`names`](Self::names) deterministically ordered for listing.
#[derive(Clone, Default)]
pub struct JobRegistry {
    jobs: BTreeMap<String, Arc<dyn Job>>,
}

impl std::fmt::Debug for JobRegistry {
    /// A `dyn Job` isn't `Debug`, so show the registry by its job names — enough
    /// for diagnostics and for `Result::unwrap_err` in tests.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRegistry")
            .field("jobs", &self.names())
            .finish()
    }
}

impl JobRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `job`, failing if its name is already taken (strict so a typo
    /// can't silently shadow a real job). Returns the registry for chaining.
    ///
    /// # Errors
    /// Returns [`JobError::Duplicate`] if a job with the same name exists.
    pub fn register(mut self, job: Arc<dyn Job>) -> Result<Self, JobError> {
        let name = job.name().to_owned();
        if self.jobs.contains_key(&name) {
            return Err(JobError::Duplicate(name));
        }
        self.jobs.insert(name, job);
        Ok(self)
    }

    /// Look up a job by name.
    ///
    /// # Errors
    /// Returns [`JobError::Unknown`] if no job is registered under `name`.
    pub fn get(&self, name: &str) -> Result<Arc<dyn Job>, JobError> {
        self.jobs
            .get(name)
            .cloned()
            .ok_or_else(|| JobError::Unknown(name.to_owned()))
    }

    /// The registered job names, in sorted order.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.jobs.keys().cloned().collect()
    }

    /// How many jobs are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether no jobs are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}
