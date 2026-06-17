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
use crate::event::{Activity, Event, EventBus, LiveEvent, run_history};
use crate::init_schema::init_schema;
use crate::run::{
    Run, cancel_run, create_run, get_run, list_run_tasks, list_runs, mark_started,
};
use crate::secret::{
    Cipher, SecretEnv, SecretMeta, delete_secret, list_secrets, put_secret, secret_env,
};
use crate::template::{
    Template, create_template, delete_template, get_template, list_templates,
};
use crate::task::{
    Status, Task, TaskEdit, Transition, create_task, delete_task, get_task, list_tasks,
    newly_ready, record_heartbeat, relate_dep, transition_task, unrelate_dep, update_task,
    upsert_task,
};

/// A cloneable handle to the durable store.
#[derive(Clone)]
pub struct StoreHandle {
    db: Arc<Surreal<Db>>,
    /// Derived from the run's master key; seals/opens secret values at rest.
    cipher: Cipher,
    /// Live tap on the run log: every recorded transition is published here for
    /// SSE subscribers. The durable `event` rows remain the source of truth.
    bus: EventBus,
}

impl StoreHandle {
    /// Open the engine, select the namespace/database, and run schema init.
    ///
    /// `master_key` derives the AES-GCM key that protects stored secrets at rest
    /// (the daemon's `LAZYBONES_SECRET_KEY`). It is never persisted; only its
    /// derived key encrypts/decrypts the `secret` table's blobs.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the engine cannot be
    /// opened, the namespace/database cannot be selected, or schema init fails.
    pub async fn open(
        engine: &StoreEngine,
        namespace: &str,
        database: &str,
        master_key: &str,
    ) -> Result<Self> {
        let db = open_engine(engine).await?;
        use_namespace(&db, namespace, database).await?;
        init_schema(&db).await?;
        Ok(Self {
            db: Arc::new(db),
            cipher: Cipher::from_master(master_key),
            bus: EventBus::new(),
        })
    }

    /// Subscribe to the live feed (the SSE `/stream` tap).
    ///
    /// The receiver sees every transition and activity published after it
    /// subscribes; it does not replay history (that is `GET /runs/:id`).
    #[must_use]
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<LiveEvent> {
        self.bus.subscribe()
    }

    /// Publish an ephemeral agent progress message on the live feed.
    ///
    /// Unlike a transition, this is not persisted — it is a "the agent is working
    /// right now" signal for the dashboard. A send with no SSE subscribers is a
    /// no-op. `run` groups it with the task's run; `at` is stamped now.
    pub fn report_activity(&self, run: &str, task: &str, actor: &str, message: &str) {
        let at = surrealdb::types::Datetime::now().to_string();
        self.bus.publish(LiveEvent::Activity(Activity {
            run: run.to_owned(),
            task: task.to_owned(),
            actor: actor.to_owned(),
            message: message.to_owned(),
            at,
        }));
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

    /// Create a new task document, failing if its id is already taken.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the id exists or the write
    /// fails.
    pub async fn create_task(&self, task: &Task) -> Result<Task> {
        create_task(&self.db, task).await
    }

    /// Overwrite a task's authored fields, preserving its lifecycle state.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the task is missing or the
    /// write fails.
    pub async fn update_task(&self, id: &str, edit: TaskEdit) -> Result<Task> {
        update_task(&self.db, id, edit).await
    }

    /// Delete a task and tear down its `depends_on` edges. Returns whether it
    /// existed.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the delete fails.
    pub async fn delete_task(&self, id: &str) -> Result<bool> {
        delete_task(&self.db, id).await
    }

    /// Relate `task ->depends_on-> dep`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn relate_dep(&self, task: &str, dep: &str) -> Result<()> {
        relate_dep(&self.db, task, dep).await
    }

    /// Drop the `task ->depends_on-> dep` edge (idempotent).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the delete fails.
    pub async fn unrelate_dep(&self, task: &str, dep: &str) -> Result<()> {
        unrelate_dep(&self.db, task, dep).await
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
        let (task, event) = transition_task(&self.db, id, transition, actor).await?;
        self.bus.publish(LiveEvent::Transition(event));
        Ok(task)
    }

    /// Read the full event history for a run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn run_history(&self, run: &str) -> Result<Vec<Event>> {
        run_history(&self.db, run).await
    }

    /// The current time as an RFC3339 string — the one timestamp source callers
    /// (the API) use to stamp `created_at`/`started_at` so they need not depend
    /// on SurrealDB directly.
    #[must_use]
    pub fn now(&self) -> String {
        surrealdb::types::Datetime::now().to_string()
    }

    /// Create a task template, failing if its id is already taken.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the id exists or the write
    /// fails.
    pub async fn create_template(&self, template: &Template) -> Result<Template> {
        create_template(&self.db, template).await
    }

    /// Read a single template by id.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_template(&self, id: &str) -> Result<Option<Template>> {
        get_template(&self.db, id).await
    }

    /// List every task template.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn list_templates(&self) -> Result<Vec<Template>> {
        list_templates(&self.db).await
    }

    /// Delete a template by id. Returns whether one existed.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the delete fails.
    pub async fn delete_template(&self, id: &str) -> Result<bool> {
        delete_template(&self.db, id).await
    }

    /// Create a workflow (run), failing if its id is already taken.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the id exists or the write
    /// fails.
    pub async fn create_run(&self, run: &Run) -> Result<Run> {
        create_run(&self.db, run).await
    }

    /// Read a single workflow (run) by id.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_run(&self, id: &str) -> Result<Option<Run>> {
        get_run(&self.db, id).await
    }

    /// List every workflow (run).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn list_runs(&self) -> Result<Vec<Run>> {
        list_runs(&self.db).await
    }

    /// List the tasks linked to workflow `run_id` (via the `run_id` FK).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn list_run_tasks(&self, run_id: &str) -> Result<Vec<Task>> {
        list_run_tasks(&self.db, run_id).await
    }

    /// Stamp a workflow's `started_at` (idempotent). Returns the run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the run is missing or the
    /// write fails.
    pub async fn mark_run_started(&self, id: &str, now: &str) -> Result<Run> {
        mark_started(&self.db, id, now).await
    }

    /// Set a workflow's lifecycle to `cancelled`. Returns the updated run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the run is missing or the
    /// write fails.
    pub async fn cancel_run(&self, id: &str) -> Result<Run> {
        cancel_run(&self.db, id).await
    }

    /// Seal and store an agent CLI credential for `tool` under `env_var`.
    /// Idempotent on `tool` — re-writing rotates the value.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if encryption or the write fails.
    pub async fn put_secret(
        &self,
        tool: &str,
        env_var: &str,
        value: &str,
    ) -> Result<SecretMeta> {
        put_secret(&self.db, &self.cipher, tool, env_var, value).await
    }

    /// List stored secrets as metadata only (no plaintext values).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn list_secrets(&self) -> Result<Vec<SecretMeta>> {
        list_secrets(&self.db).await
    }

    /// Delete the credential for `tool`. Returns whether one existed.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the delete fails.
    pub async fn delete_secret(&self, tool: &str) -> Result<bool> {
        delete_secret(&self.db, tool).await
    }

    /// Decrypt every secret into `env_var → value` pairs — the loop's export.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails or a blob
    /// cannot be decrypted (wrong master key).
    pub async fn secret_env(&self) -> Result<Vec<SecretEnv>> {
        secret_env(&self.db, &self.cipher).await
    }
}
