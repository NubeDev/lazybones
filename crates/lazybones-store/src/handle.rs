//! The durable read/write boundary handle for a lazybones run.
//!
//! Owns the open, bootstrapped, schema-initialised SurrealDB connection and
//! exposes the task/event/dependency verbs the REST surface drives. Cloning is
//! cheap (an `Arc` bump) so axum can share it across handlers.

use std::sync::Arc;

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::bootstrap::use_namespace;
use crate::check_health::probe;
use crate::connect::{StoreEngine, open_engine};
use crate::error::Result;
use crate::event::{Event, run_history};
use crate::init_schema::init_schema;
use crate::task::{
    Status, Task, Transition, get_task, list_tasks, newly_ready, record_heartbeat, relate_dep,
    transition_task, upsert_task,
};

/// A cloneable handle to the durable store.
#[derive(Clone)]
pub struct StoreHandle {
    db: Arc<Surreal<Db>>,
}

impl StoreHandle {
    /// Open the engine, select the namespace/database, and run schema init.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the engine cannot be
    /// opened, the namespace/database cannot be selected, or schema init fails.
    pub async fn open(engine: &StoreEngine, namespace: &str, database: &str) -> Result<Self> {
        let db = open_engine(engine).await?;
        use_namespace(&db, namespace, database).await?;
        init_schema(&db).await?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Probe the underlying engine for liveness.
    ///
    /// # Errors
    /// Returns [`StoreError::Health`](crate::StoreError::Health) if the engine
    /// does not answer.
    pub async fn health(&self) -> Result<()> {
        probe(&self.db).await
    }

    /// Idempotently upsert a task document (the workfile-sync write).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn upsert_task(&self, task: &Task) -> Result<Task> {
        upsert_task(&self.db, task).await
    }

    /// Relate `task ->depends_on-> dep`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn relate_dep(&self, task: &str, dep: &str) -> Result<()> {
        relate_dep(&self.db, task, dep).await
    }

    /// Read a single task by concept id.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_task(&self, id: &str) -> Result<Option<Task>> {
        get_task(&self.db, id).await
    }

    /// List tasks, optionally narrowed by status.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn list_tasks(&self, status: Option<Status>) -> Result<Vec<Task>> {
        list_tasks(&self.db, status).await
    }

    /// The concept ids of `pending` tasks whose deps are all `done`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn newly_ready(&self) -> Result<Vec<String>> {
        newly_ready(&self.db).await
    }

    /// Stamp a running task with a fresh heartbeat. Returns `false` if absent.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn heartbeat(&self, id: &str) -> Result<bool> {
        record_heartbeat(&self.db, id).await
    }

    /// Apply a validated lifecycle transition, recording an event.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) on an illegal transition, a
    /// missing task, or a write failure.
    pub async fn transition(&self, id: &str, transition: Transition, actor: &str) -> Result<Task> {
        transition_task(&self.db, id, transition, actor).await
    }

    /// Read the full event history for a run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn run_history(&self, run: &str) -> Result<Vec<Event>> {
        run_history(&self.db, run).await
    }
}
