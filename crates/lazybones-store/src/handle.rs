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
use crate::agent::{
    AgentCatalog, AgentCatalogEdit, create_agent, delete_agent, get_agent, list_agents,
    seed_default_agents, update_agent,
};
use crate::agent_chat::{
    AgentConversation, AgentMessage, AgentRole, ConfirmAction, agent_message_history,
    append_agent_message, append_confirm_request, create_agent_conversation,
    get_agent_conversation, list_agent_conversations,
};
use crate::attachment::{Attachment, attach, detach, list_attachments};
use crate::management_agent::{
    ManagementAgentConfig, ManagementAgentScope, delete_management_agent_scoped,
    get_management_agent, get_management_agent_resolved, get_management_agent_scoped,
    put_management_agent, put_management_agent_scoped,
};
use crate::preferences::{Preferences, get_preferences, put_preferences};
use crate::skill::{
    Skill, create_skill, delete_skill, get_skill, list_skills, seed_default_skills, update_skill,
};
use crate::chat::{ChatMessage, ChatRole, append_chat, chat_history};
use crate::connect::{StoreEngine, open_engine};
use crate::error::Result;
use crate::event::{Activity, Event, EventBus, LiveEvent, run_history};
use crate::follow_up::{
    FollowUp, FollowUpFilter, NewFollowUpEntry, file_follow_up, resolve_follow_up, run_follow_ups,
};
use crate::hcom_log::{
    HcomLogEntry, HcomLogFilter, NewHcomLogEntry, append_hcom_log, run_hcom_log,
};
use crate::init_schema::init_schema;
use crate::run::{
    Lifecycle, Run, Workspace, advance_hcom_cursor, create_run, delete_run, get_run,
    clear_started, list_run_tasks, list_runs, mark_started, resume_run, set_pr_url, stop_run,
    update_workspace,
};
use crate::secret::{
    Cipher, SecretEnv, SecretMeta, delete_secret, list_secrets, put_secret, secret_env,
};
use crate::template::{
    Template, create_template, delete_template, get_template, list_templates, update_template,
};
use crate::task::{
    RetryStrategy, Status, Task, TaskEdit, Transition, bump_retry_count, create_task, delete_task,
    get_task, list_tasks, newly_ready, record_heartbeat, relate_dep, reset_task, set_issue_link,
    set_retry_policy, transition_task, unrelate_dep, update_task, upsert_task,
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

    /// Persist a task's GitHub-issue linkage (`issue_url`,
    /// `issue_close_on_done`, `issue_synced_state`), preserving its lifecycle and
    /// authored fields. The engine's issue actions + reverse-sync poll write here.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the task is missing or the
    /// write fails.
    pub async fn set_issue_link(&self, task: &Task) -> Result<Task> {
        set_issue_link(&self.db, task).await
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

    /// The concept ids of `pending` tasks whose deps are all `done`, **excluding**
    /// any task whose parent run is in `excluded` (a paused or not-yet-started
    /// workflow promotes nothing). Standalone tasks (no `run_id`) are always
    /// eligible. Pass the ids from [`unpromotable_run_ids`](Self::unpromotable_run_ids),
    /// or `&[]` to promote freely.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn newly_ready(&self, stopped_runs: &[String]) -> Result<Vec<String>> {
        newly_ready(&self.db, stopped_runs).await
    }

    /// The ids of every run the scheduler must **not** promote or claim for: a
    /// `stopped` (paused) workflow, or an `active` one that has never been started
    /// (`started_at` is `null`). Creating a workflow must not run it — only an
    /// explicit `start` (which stamps `started_at`) does. Cheap (one `list_runs`).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn unpromotable_run_ids(&self) -> Result<Vec<String>> {
        Ok(list_runs(&self.db)
            .await?
            .into_iter()
            .filter(|r| r.lifecycle == Lifecycle::Stopped || r.started_at.is_none())
            .map(|r| r.id)
            .collect())
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

    /// Force a task back to `pending`, clearing its per-run fields (the restart
    /// override — bypasses the lifecycle state machine). Records and publishes a
    /// `<from> -> pending` event. Killing the agent / removing the worktree is the
    /// caller's job.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) on a missing task or a write
    /// failure.
    pub async fn reset(&self, id: &str, actor: &str) -> Result<Task> {
        let (task, event) = reset_task(&self.db, id, actor).await?;
        self.bus.publish(LiveEvent::Transition(event));
        Ok(task)
    }

    /// Set (or clear with `strategy = None`) a task's hands-off auto-retry policy.
    /// `max_retries = None` leaves the existing cap unchanged. Returns the updated
    /// task. This is operator config — it never touches lifecycle state.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) on a missing task or a write
    /// failure.
    pub async fn set_retry_policy(
        &self,
        id: &str,
        strategy: Option<RetryStrategy>,
        max_retries: Option<u32>,
    ) -> Result<Task> {
        set_retry_policy(&self.db, id, strategy, max_retries).await
    }

    /// Revive a *blocked* task with operator/strategy guidance: append `guidance`
    /// to the task's conversation (as a `User` message, so
    /// [`prompt::compose`](../../lazybones_engine/scheduler/prompt/fn.compose.html)
    /// folds it into the re-spawn prompt), then apply the `blocked -> ready`
    /// [`Revive`](Transition::Revive) edge. The worktree is kept, so the
    /// re-spawned agent resumes in place with the guidance in view.
    ///
    /// When `bump_count` is set, the task's spent-auto-retry counter is
    /// incremented first — the scheduler passes `true` for a hands-off retry so
    /// the cap is enforced; a human-driven retry passes `false` (unbounded, a
    /// person is in the loop). Returns the revived task.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) on a missing task, an illegal
    /// transition (the task wasn't `blocked`), or a write failure.
    pub async fn revive_with_guidance(
        &self,
        id: &str,
        guidance: &str,
        actor: &str,
        bump_count: bool,
    ) -> Result<Task> {
        // Group the chat row under the workflow id (its FK) when present, falling
        // back to the event-grouping `run` label — mirroring the chat route.
        let task = get_task(&self.db, id)
            .await?
            .ok_or_else(|| crate::StoreError::TaskNotFound(id.to_owned()))?;
        let run = task.run_id.clone().unwrap_or_else(|| task.run.clone());

        let message = append_chat(&self.db, &run, id, ChatRole::User, guidance, None).await?;
        self.bus.publish(LiveEvent::Chat(message));

        if bump_count {
            bump_retry_count(&self.db, id).await?;
        }

        // The status edge stays validated: only a `blocked` task can be revived.
        let (task, event) = transition_task(&self.db, id, Transition::Revive, actor).await?;
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

    /// Append one message to a task's conversation and publish it on the live bus
    /// for SSE subscribers. `hcom_id` is `None` for an operator message and the
    /// source hcom event id for a mirrored agent reply (deduped on `(task,
    /// hcom_id)`). Persistence happens before publish, so anything streamed is
    /// already re-fetchable via [`chat_history`](Self::chat_history).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn append_chat(
        &self,
        run: &str,
        task: &str,
        role: ChatRole,
        text: &str,
        hcom_id: Option<i64>,
    ) -> Result<ChatMessage> {
        let message = append_chat(&self.db, run, task, role, text, hcom_id).await?;
        self.bus.publish(LiveEvent::Chat(message.clone()));
        Ok(message)
    }

    /// Read a task's full conversation, oldest first.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn chat_history(&self, task: &str) -> Result<Vec<ChatMessage>> {
        chat_history(&self.db, task).await
    }

    /// Read the single Lazybones-Agent configuration, or `None` if unset.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_management_agent(&self) -> Result<Option<ManagementAgentConfig>> {
        get_management_agent(&self.db).await
    }

    /// Write the single Lazybones-Agent configuration, returning it as stored.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn put_management_agent(
        &self,
        config: &ManagementAgentConfig,
    ) -> Result<ManagementAgentConfig> {
        put_management_agent(&self.db, config).await
    }

    /// Read the config stored at exactly `scope` (no fallback), or `None`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_management_agent_scoped(
        &self,
        scope: &ManagementAgentScope,
    ) -> Result<Option<ManagementAgentConfig>> {
        get_management_agent_scoped(&self.db, scope).await
    }

    /// Resolve the effective config for `scope`: its own override if set, else
    /// the global record (`workflow-override ?? global`, scope §11 Q1).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if a read fails.
    pub async fn get_management_agent_resolved(
        &self,
        scope: &ManagementAgentScope,
    ) -> Result<Option<ManagementAgentConfig>> {
        get_management_agent_resolved(&self.db, scope).await
    }

    /// Write the config at `scope`, returning it as stored.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn put_management_agent_scoped(
        &self,
        scope: &ManagementAgentScope,
        config: &ManagementAgentConfig,
    ) -> Result<ManagementAgentConfig> {
        put_management_agent_scoped(&self.db, scope, config).await
    }

    /// Delete the config override at `scope`, returning whether it existed.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the delete fails.
    pub async fn delete_management_agent_scoped(
        &self,
        scope: &ManagementAgentScope,
    ) -> Result<bool> {
        delete_management_agent_scoped(&self.db, scope).await
    }

    /// Read the single global user-preferences record, or `None` if unset.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_preferences(&self) -> Result<Option<Preferences>> {
        get_preferences(&self.db).await
    }

    /// Write the single global user-preferences record, returning it as stored.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn put_preferences(&self, prefs: &Preferences) -> Result<Preferences> {
        put_preferences(&self.db, prefs).await
    }

    /// Open a new management-agent conversation, optionally snapshotting the
    /// page context it was started on.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn create_agent_conversation(
        &self,
        page_context: Option<&serde_json::Value>,
    ) -> Result<AgentConversation> {
        create_agent_conversation(&self.db, page_context, &self.now()).await
    }

    /// Read a single management-agent conversation by id.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_agent_conversation(&self, id: &str) -> Result<Option<AgentConversation>> {
        get_agent_conversation(&self.db, id).await
    }

    /// List all management-agent conversations, newest first.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn list_agent_conversations(&self) -> Result<Vec<AgentConversation>> {
        list_agent_conversations(&self.db).await
    }

    /// Append one message to a management-agent conversation and publish it on
    /// the live bus for the per-conversation SSE stream. Mirrored agent replies
    /// (with `hcom_id`) are deduped on `(conversation_id, hcom_id)`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn append_agent_message(
        &self,
        conversation_id: &str,
        role: AgentRole,
        text: &str,
        hcom_id: Option<i64>,
    ) -> Result<AgentMessage> {
        let message =
            append_agent_message(&self.db, conversation_id, role, text, hcom_id).await?;
        self.bus.publish(LiveEvent::AgentMessage(message.clone()));
        Ok(message)
    }

    /// Publish an ephemeral management-agent *activity* tick on the live bus for
    /// the per-conversation SSE — "what the agent is doing right now". NOT
    /// persisted (a no-op for reloaded history); pure live feedback.
    pub fn report_agent_activity(&self, conversation_id: &str, text: &str) {
        self.bus.publish(LiveEvent::AgentActivity {
            conversation_id: conversation_id.to_owned(),
            text: text.to_owned(),
        });
    }

    /// Append a gated `confirm` message proposing `action` and publish it on the
    /// live bus for the per-conversation stream. The agent only *proposes*; the
    /// UI issues the call under the operator's token (scope §10.2).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn append_confirm_request(
        &self,
        conversation_id: &str,
        summary: &str,
        action: &ConfirmAction,
        hcom_id: Option<i64>,
    ) -> Result<AgentMessage> {
        let message =
            append_confirm_request(&self.db, conversation_id, summary, action, hcom_id).await?;
        self.bus.publish(LiveEvent::AgentMessage(message.clone()));
        Ok(message)
    }

    /// Read a management-agent conversation's full history, oldest first.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn agent_message_history(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<AgentMessage>> {
        agent_message_history(&self.db, conversation_id).await
    }

    /// Append one raw hcom event to the durable hcom log (idempotent on
    /// `(run, hcom_id)`) and publish it on the live bus for SSE subscribers.
    ///
    /// Persistence happens *before* publish, so anything streamed is already
    /// re-fetchable via [`run_hcom_log`](Self::run_hcom_log).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn append_hcom_log(&self, entry: NewHcomLogEntry) -> Result<HcomLogEntry> {
        let stored = append_hcom_log(&self.db, entry).await?;
        self.bus.publish(LiveEvent::HcomLog(stored.clone()));
        Ok(stored)
    }

    /// Read a run's durable hcom log, oldest first, narrowed by `filter`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn run_hcom_log(
        &self,
        run: &str,
        filter: &HcomLogFilter,
    ) -> Result<Vec<HcomLogEntry>> {
        run_hcom_log(&self.db, run, filter).await
    }

    /// Advance a run's `hcom_log_cursor` to `max(current, cursor)` (monotonic).
    /// Returns the updated run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the run is missing or the
    /// write fails.
    pub async fn advance_hcom_cursor(&self, id: &str, cursor: u64) -> Result<Run> {
        advance_hcom_cursor(&self.db, id, cursor).await
    }

    /// File a follow-up — a durable "a human needs to act" note — idempotent on
    /// `(run, dedup_key)`. The scheduler calls this when it hits a wall it can't
    /// clear (a consent screen, a missing credential, a repeated spawn failure);
    /// agents call it via `POST /follow-ups` to flag something for review.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn file_follow_up(&self, entry: NewFollowUpEntry) -> Result<FollowUp> {
        file_follow_up(&self.db, entry).await
    }

    /// A run's follow-ups, most-recently-updated first, narrowed by `filter`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn run_follow_ups(
        &self,
        run: &str,
        filter: &FollowUpFilter,
    ) -> Result<Vec<FollowUp>> {
        run_follow_ups(&self.db, run, filter).await
    }

    /// Mark one follow-up `resolved` by id; `None` if no such row.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the write fails.
    pub async fn resolve_follow_up(&self, id: &str) -> Result<Option<FollowUp>> {
        resolve_follow_up(&self.db, id).await
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

    /// Edit an existing template, failing if its id is unknown. Preserves the
    /// original `created_at`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if no template with that id
    /// exists or the write fails.
    pub async fn update_template(&self, template: &Template) -> Result<Template> {
        update_template(&self.db, template).await
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

    /// Create a skill, failing if its id is already taken.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the id exists or the write
    /// fails.
    pub async fn create_skill(&self, skill: &Skill) -> Result<Skill> {
        create_skill(&self.db, skill).await
    }

    /// Edit an existing skill, failing if its id is unknown. Preserves the
    /// original `created_at`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if no skill with that id exists
    /// or the write fails.
    pub async fn update_skill(&self, skill: &Skill) -> Result<Skill> {
        update_skill(&self.db, skill).await
    }

    /// Read a single skill by id.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_skill(&self, id: &str) -> Result<Option<Skill>> {
        get_skill(&self.db, id).await
    }

    /// List every skill.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn list_skills(&self) -> Result<Vec<Skill>> {
        list_skills(&self.db).await
    }

    /// Delete a skill by id. Returns whether one existed. Does not cascade to its
    /// attachments (they carry no hard FK).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the delete fails.
    pub async fn delete_skill(&self, id: &str) -> Result<bool> {
        delete_skill(&self.db, id).await
    }

    /// Seed the bundled demo skill catalogue, never clobbering existing ids.
    /// Returns how many skills were newly created.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the bundled YAML can't be
    /// parsed or a write fails.
    pub async fn seed_default_skills(&self, now: &str) -> Result<usize> {
        seed_default_skills(&self.db, now).await
    }

    /// Attach a polymorphic thing `(thing_kind, thing_id)` to an owner
    /// `(owner_kind, owner_id)`. Idempotent. See [`attachment`](crate::attachment).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read or write fails.
    pub async fn attach(
        &self,
        owner_kind: &str,
        owner_id: &str,
        thing_kind: &str,
        thing_id: &str,
    ) -> Result<Attachment> {
        attach(&self.db, owner_kind, owner_id, thing_kind, thing_id).await
    }

    /// Detach a thing from an owner. Returns whether the link existed.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn detach(
        &self,
        owner_kind: &str,
        owner_id: &str,
        thing_kind: &str,
        thing_id: &str,
    ) -> Result<bool> {
        detach(&self.db, owner_kind, owner_id, thing_kind, thing_id).await
    }

    /// List an owner's attachments, optionally narrowed to one `thing_kind`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn list_attachments(
        &self,
        owner_kind: &str,
        owner_id: &str,
        thing_kind: Option<&str>,
    ) -> Result<Vec<Attachment>> {
        list_attachments(&self.db, owner_kind, owner_id, thing_kind).await
    }

    /// Seed the bundled default agent catalog, never clobbering existing ids.
    /// Returns how many entries were newly created.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the bundled YAML can't be
    /// parsed or a write fails.
    pub async fn seed_default_agents(&self, now: &str) -> Result<usize> {
        seed_default_agents(&self.db, now).await
    }

    /// Create an agent catalog entry, failing if its id is already taken.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the id exists or the write
    /// fails.
    pub async fn create_agent(&self, agent: &AgentCatalog) -> Result<AgentCatalog> {
        create_agent(&self.db, agent).await
    }

    /// Read a single agent catalog entry by id.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the read fails.
    pub async fn get_agent(&self, id: &str) -> Result<Option<AgentCatalog>> {
        get_agent(&self.db, id).await
    }

    /// List every agent catalog entry (ordered by id).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the query fails.
    pub async fn list_agents(&self) -> Result<Vec<AgentCatalog>> {
        list_agents(&self.db).await
    }

    /// Update an agent catalog entry's authored fields, bumping `updated_at`.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if no such agent exists or the
    /// write fails.
    pub async fn update_agent(
        &self,
        id: &str,
        edit: AgentCatalogEdit,
        now: &str,
    ) -> Result<AgentCatalog> {
        update_agent(&self.db, id, edit, now).await
    }

    /// Delete an agent catalog entry by id. Returns whether one existed.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the delete fails.
    pub async fn delete_agent(&self, id: &str) -> Result<bool> {
        delete_agent(&self.db, id).await
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

    /// Overwrite a workflow's workspace defaults (the inheritable git + agent
    /// config), keeping its `repo`, lifecycle and timestamps. Returns the run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the run is missing or the
    /// write fails.
    pub async fn update_workspace(&self, id: &str, workspace: Workspace) -> Result<Run> {
        update_workspace(&self.db, id, workspace).await
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

    /// Un-activate a workflow: clear `started_at` and force lifecycle `Active`, so
    /// a restart returns it to the `draft`-equivalent state the scheduler skips
    /// until the operator presses Start again. Returns the run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the run is missing or the
    /// write fails.
    pub async fn clear_run_started(&self, id: &str) -> Result<Run> {
        clear_started(&self.db, id).await
    }

    /// Record the auto-opened PR url on a workflow (the auto-PR idempotency guard).
    /// Returns the run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the run is missing or the
    /// write fails.
    pub async fn set_run_pr_url(&self, id: &str, url: &str) -> Result<Run> {
        set_pr_url(&self.db, id, url).await
    }

    /// Pause a workflow: set its lifecycle to `stopped`. Returns the updated run.
    /// Reversible via [`resume_run`](Self::resume_run); the scheduler promotes and
    /// claims nothing for a stopped run.
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the run is missing or the
    /// write fails.
    pub async fn stop_run(&self, id: &str) -> Result<Run> {
        stop_run(&self.db, id).await
    }

    /// Un-pause a workflow: set its lifecycle back to `active`. Returns the
    /// updated run. The reverse of [`stop_run`](Self::stop_run).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if the run is missing or the
    /// write fails.
    pub async fn resume_run(&self, id: &str) -> Result<Run> {
        resume_run(&self.db, id).await
    }

    /// Hard-delete a workflow and the tasks linked to it. Returns whether the
    /// workflow existed. Unlike [`stop_run`](Self::stop_run), this removes
    /// the record; cascades to its tasks (each with its `depends_on` edges).
    ///
    /// # Errors
    /// Returns a [`StoreError`](crate::StoreError) if any delete fails.
    pub async fn delete_run(&self, id: &str) -> Result<bool> {
        delete_run(&self.db, id).await
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
