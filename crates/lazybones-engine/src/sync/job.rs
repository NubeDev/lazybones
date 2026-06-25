//! [`Job`] wrappers so content sync runs through the generic job service — driven
//! synchronously from an API request or spawned in the background by the daemon.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use lazybones_jobs::{Job, JobError, JobReport};
use lazybones_store::{BlobStore, StoreHandle};

use super::actions;

/// The registry name of the pull job.
pub const PULL_JOB: &str = "content-sync-pull";
/// The registry name of the push job.
pub const PUSH_JOB: &str = "content-sync-push";

/// Pull the remote and import it into the store. Owns the handles it needs so the
/// runner stays generic — including the blob store, so document images are
/// imported alongside their metadata.
pub struct PullJob {
    store: StoreHandle,
    blobs: Arc<dyn BlobStore>,
    data_dir: PathBuf,
}

impl PullJob {
    /// Build the pull job over `store` + `blobs`, deriving the checkout under
    /// `data_dir`.
    #[must_use]
    pub fn new(
        store: StoreHandle,
        blobs: Arc<dyn BlobStore>,
        data_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            store,
            blobs,
            data_dir: data_dir.into(),
        }
    }
}

#[async_trait]
impl Job for PullJob {
    fn name(&self) -> &str {
        PULL_JOB
    }

    async fn run(&self) -> Result<JobReport, JobError> {
        let counts = actions::pull(&self.store, self.blobs.as_ref(), &self.data_dir)
            .await
            .map_err(|e| JobError::failed(PULL_JOB, e))?;
        Ok(JobReport::new(
            PULL_JOB,
            format!(
                "pulled + imported {} records ({} images)",
                counts.total, counts.assets
            ),
        ))
    }
}

/// Export the store and push it to the remote (including document images).
pub struct PushJob {
    store: StoreHandle,
    blobs: Arc<dyn BlobStore>,
    data_dir: PathBuf,
}

impl PushJob {
    /// Build the push job over `store` + `blobs`, deriving the checkout under
    /// `data_dir`.
    #[must_use]
    pub fn new(
        store: StoreHandle,
        blobs: Arc<dyn BlobStore>,
        data_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            store,
            blobs,
            data_dir: data_dir.into(),
        }
    }
}

#[async_trait]
impl Job for PushJob {
    fn name(&self) -> &str {
        PUSH_JOB
    }

    async fn run(&self) -> Result<JobReport, JobError> {
        let outcome = actions::push(&self.store, self.blobs.as_ref(), &self.data_dir)
            .await
            .map_err(|e| JobError::failed(PUSH_JOB, e))?;
        let summary = if outcome.pushed {
            format!("pushed {} records", outcome.exported.total)
        } else {
            "clean — nothing to push".to_owned()
        };
        Ok(JobReport::new(PUSH_JOB, summary))
    }
}
