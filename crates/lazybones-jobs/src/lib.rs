//! A small, generic **job service**: a registry of named units of background work
//! and a runner that executes them either synchronously or asynchronously.
//!
//! It exists so recurring side-effecting work — content sync to a git repo today,
//! a Google Drive flavour or anything else tomorrow — has one place to live and
//! one way to be invoked, rather than each feature hand-rolling its own
//! `tokio::spawn` + error handling. The pieces:
//!
//! - [`Job`] — the trait one job implements: a `name` and an async `run` that
//!   returns a [`JobReport`]. A job owns its own handles (store, sync repo).
//! - [`JobRegistry`] — a `name → Job` map, built once at startup.
//! - [`JobRunner`] — runs a job by name: [`run_now`](JobRunner::run_now) awaits
//!   the result (sync); [`spawn`](JobRunner::spawn) returns a [`JobHandle`]
//!   immediately (async).
//!
//! It deliberately reuses Tokio for execution instead of inventing an executor,
//! and keeps no persistent state — a durable, store-backed queue can be layered
//! later by draining the same [`Job`]s. This crate has no dependency on the store
//! or gh crates: concrete jobs that wire those in live next to the code that owns
//! those handles.
//!
//! ```no_run
//! use std::sync::Arc;
//! use lazybones_jobs::{Job, JobRegistry, JobReport, JobRunner, JobError};
//! use async_trait::async_trait;
//!
//! struct Ping;
//! #[async_trait]
//! impl Job for Ping {
//!     fn name(&self) -> &str { "ping" }
//!     async fn run(&self) -> Result<JobReport, JobError> {
//!         Ok(JobReport::new("ping", "pong"))
//!     }
//! }
//!
//! # async fn demo() -> Result<(), JobError> {
//! let registry = JobRegistry::new().register(Arc::new(Ping))?;
//! let runner = JobRunner::new(registry);
//! let report = runner.run_now("ping").await?;       // synchronous
//! let handle = runner.spawn("ping")?;               // asynchronous
//! let _ = handle.join().await?;
//! # Ok(()) }
//! ```

mod error;
mod job;
mod registry;
mod runner;

pub use error::JobError;
pub use job::{Job, JobReport};
pub use registry::JobRegistry;
pub use runner::{JobHandle, JobRunner};

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;

    /// A job that counts how many times it ran and reports a fixed summary.
    struct Counter {
        name: String,
        runs: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Job for Counter {
        fn name(&self) -> &str {
            &self.name
        }
        async fn run(&self) -> Result<JobReport, JobError> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            Ok(JobReport::new(&self.name, "ok"))
        }
    }

    /// A job that always fails, to exercise the error path.
    struct Boom;
    #[async_trait]
    impl Job for Boom {
        fn name(&self) -> &str {
            "boom"
        }
        async fn run(&self) -> Result<JobReport, JobError> {
            Err(JobError::failed("boom", "kaboom"))
        }
    }

    fn runner_with(jobs: Vec<Arc<dyn Job>>) -> JobRunner {
        let mut reg = JobRegistry::new();
        for j in jobs {
            reg = reg.register(j).unwrap();
        }
        JobRunner::new(reg)
    }

    #[tokio::test]
    async fn run_now_executes_synchronously_and_returns_report() {
        let runs = Arc::new(AtomicUsize::new(0));
        let runner = runner_with(vec![Arc::new(Counter {
            name: "sync-job".into(),
            runs: runs.clone(),
        })]);

        let report = runner.run_now("sync-job").await.unwrap();
        assert_eq!(report.summary, "ok");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn spawn_runs_in_the_background_and_handle_joins() {
        let runs = Arc::new(AtomicUsize::new(0));
        let runner = runner_with(vec![Arc::new(Counter {
            name: "async-job".into(),
            runs: runs.clone(),
        })]);

        let handle = runner.spawn("async-job").unwrap();
        assert_eq!(handle.name(), "async-job");
        let report = handle.join().await.unwrap();
        assert_eq!(report.job, "async-job");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unknown_job_fails_fast_on_both_paths() {
        let runner = runner_with(vec![]);
        assert!(matches!(
            runner.run_now("nope").await,
            Err(JobError::Unknown(_))
        ));
        assert!(matches!(runner.spawn("nope"), Err(JobError::Unknown(_))));
    }

    #[tokio::test]
    async fn duplicate_registration_is_rejected() {
        let err = JobRegistry::new()
            .register(Arc::new(Boom))
            .unwrap()
            .register(Arc::new(Boom))
            .unwrap_err();
        assert!(matches!(err, JobError::Duplicate(n) if n == "boom"));
    }

    #[tokio::test]
    async fn job_failure_propagates_through_run_now() {
        let runner = runner_with(vec![Arc::new(Boom)]);
        let err = runner.run_now("boom").await.unwrap_err();
        assert!(matches!(err, JobError::Failed { job, .. } if job == "boom"));
    }

    #[test]
    fn registry_lists_names_sorted() {
        let reg = JobRegistry::new()
            .register(Arc::new(Counter {
                name: "b".into(),
                runs: Arc::new(AtomicUsize::new(0)),
            }))
            .unwrap()
            .register(Arc::new(Counter {
                name: "a".into(),
                runs: Arc::new(AtomicUsize::new(0)),
            }))
            .unwrap();
        assert_eq!(reg.names(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(reg.len(), 2);
    }
}
