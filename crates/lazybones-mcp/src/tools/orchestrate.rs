//! Orchestration tools — tasks, skills, templates, workflows (design §6.1).
//!
//! Authoring verbs (`workflow.create`/`add_task`, `task.create`/`update`,
//! `template.*`, `skill.*`) check `Capability::Author`; reads need none. Lifecycle
//! verbs (`workflow.start` → `Claim`; `workflow.stop`/`resume`/`restart` and
//! `task.retry`/`auto_retry`/`cancel` → `Block`) are present but gated, so the
//! default management (`Author`) token authors then hands back — it cannot start a
//! run. `follow_up.file` is the agent's "needs a human" escape hatch (design §6.1
//! marks it `read`, so the authoring agent can reach it even though the REST route
//! is `Block`-guarded).
//!
//! Each tool is a thin twin of its REST route: authenticate the bearer token to a
//! session, assert the capability via [`McpServer::authorize`], call the existing
//! `StoreHandle`/engine verb, serialize the domain type. No business logic lives
//! here — the two surfaces share one store boundary so they can never drift.

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::{tool, tool_router};
use serde_json::{Value, json};

use lazybones_auth::Capability;
use lazybones_store::{
    NewFollowUpEntry, Run, RetryStrategy, Skill, Status, Task, TaskEdit, Template, Transition,
    deps_with_reuse, derived_state, instantiate,
};

use crate::args::{
    FollowUpFileArgs, IdArgs, SkillArgs, TaskAutoRetryArgs, TaskCancelArgs, TaskCreateArgs,
    TaskRetryArgs, TaskUpdateArgs, TemplateArgs, WorkflowAddTaskArgs, WorkflowCreateArgs,
    WorkflowRestartArgs, worktree_mode_or_default, worktree_mode_override,
};
use crate::auth::authorization_header;
use crate::error::{McpError, McpResult};
use crate::server::McpServer;
use crate::tools::json;

/// Build the `GET /workflows`-style summary value for a run + its tasks — the MCP
/// twin of `WorkflowSummary`/`WorkflowDetail` (which live in `lazybones-api`'s
/// `dto`, out of reach here). Flattens the run and adds the derived state, the task
/// counts, and the task ids, so a client sees the same shape REST serves.
fn workflow_summary(run: &Run, tasks: &[Task]) -> Value {
    let mut value = serde_json::to_value(run).unwrap_or(Value::Null);
    let done = tasks.iter().filter(|t| t.status == Status::Done).count();
    let task_ids: Vec<&String> = tasks.iter().map(|t| &t.id).collect();
    if let Value::Object(map) = &mut value {
        map.insert(
            "state".into(),
            Value::from(derived_state(run.lifecycle, tasks).as_str()),
        );
        map.insert("task_count".into(), Value::from(tasks.len()));
        map.insert("done_count".into(), Value::from(done));
        map.insert(
            "task_ids".into(),
            serde_json::to_value(task_ids).unwrap_or(Value::Null),
        );
    }
    value
}

#[tool_router(router = orchestrate_router, vis = "pub(crate)")]
impl McpServer {
    /// `workflow.create` — create a workflow (an empty, `active` run) bound to a
    /// workspace. The twin of `POST /workflows`: requires `Capability::Author`, the
    /// same gate the route checks (design §6.1). Authoring is not running — the run
    /// promotes nothing until the operator starts it.
    ///
    /// Refuses with `unauthorized` when the call carries no (or an unregistered)
    /// token, and with `forbidden` (missing `author`) when the token lacks the
    /// capability. Conflicts when the id is already taken.
    #[tool(
        name = "workflow.create",
        description = "Create a workflow (an empty, active run) bound to a workspace. Requires the Author capability (twin of POST /workflows). Authoring is not running: the run promotes nothing until an operator starts it."
    )]
    pub async fn workflow_create(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<WorkflowCreateArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;
        let run = Run::new(
            &args.id,
            &args.title,
            args.workspace.into_workspace(),
            self.store().now(),
        );
        let created = self.store().create_run(&run).await.map_err(McpError::from)?;
        json(created)
    }

    /// `workflow.add_task` — add a task to a workflow, inline or `from_template`.
    /// The twin of `POST /workflows/:id/tasks`: requires `Author`. `404` if the
    /// workflow or named template is unknown; conflicts if the task id is taken.
    #[tool(
        name = "workflow.add_task",
        description = "Add a task to a workflow (inline, or instantiated from a template). Requires the Author capability (twin of POST /workflows/:id/tasks)."
    )]
    pub async fn workflow_add_task(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<WorkflowAddTaskArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;

        // The workflow must exist (404) so its tasks key off a real run.
        self.store()
            .get_run(&args.workflow_id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;

        // Build the task: instantiate from a template, or author from the args.
        let mut task = match &args.from_template {
            Some(template_id) => {
                let template = self
                    .store()
                    .get_template(template_id)
                    .await
                    .map_err(McpError::from)?
                    .ok_or(McpError::NotFound)?;
                instantiate(
                    &template,
                    &args.id,
                    &args.title,
                    self.run_label(),
                    &args.workflow_id,
                    args.deps.clone(),
                )
            }
            None => {
                let mut t = Task::seed(
                    &args.id,
                    self.run_label(),
                    &args.title,
                    &args.spec,
                    args.deps.clone(),
                    Vec::new(),
                    args.tool.clone(),
                );
                t.run_id = Some(args.workflow_id.clone());
                t
            }
        };

        // Refine with the explicit fields (these win over template defaults).
        task.owns = args.owns.clone();
        if args.tool.is_some() {
            task.tool = args.tool.clone();
        }
        task.model = args.model.clone();
        task.effort = args.effort.clone();
        if let Some(mode) = worktree_mode_override(args.worktree_mode_override.clone()) {
            task.worktree_mode_override = Some(mode);
        }
        task.reuse_from = args.reuse_from.clone();

        // `reuse_from` implies a dep on the source task (its worktree must exist
        // first). Fold a *known* source into the dep set so the graph orders them;
        // an unknown source stays out and the claim-time guard handles it — mirroring
        // the REST route exactly.
        let source_known = match &args.reuse_from {
            Some(src) => self
                .store()
                .get_task(src)
                .await
                .map_err(McpError::from)?
                .is_some(),
            None => false,
        };
        let deps = if source_known {
            deps_with_reuse(&args.deps, args.reuse_from.as_deref())
        } else {
            args.deps.clone()
        };
        task.deps = deps.clone();

        let created = self.store().create_task(&task).await.map_err(McpError::from)?;
        for dep in &deps {
            self.store()
                .relate_dep(&args.id, dep)
                .await
                .map_err(McpError::from)?;
        }
        json(created)
    }

    /// `workflow.list` — every workflow with its derived state + task counts. An
    /// open read (no capability), the twin of `GET /workflows`.
    #[tool(
        name = "workflow.list",
        description = "List every workflow with its derived state and task counts. No capability required (twin of GET /workflows)."
    )]
    pub async fn workflow_list(&self) -> McpResult<Json<Value>> {
        let runs = self.store().list_runs().await.map_err(McpError::from)?;
        let mut out = Vec::with_capacity(runs.len());
        for run in runs {
            let tasks = self
                .store()
                .list_run_tasks(&run.id)
                .await
                .map_err(McpError::from)?;
            out.push(workflow_summary(&run, &tasks));
        }
        json(out)
    }

    /// `workflow.get` — one workflow's detail (workspace, derived state, counts,
    /// task ids). An open read, the twin of `GET /workflows/:id`. `404` if unknown.
    #[tool(
        name = "workflow.get",
        description = "Fetch one workflow's detail: workspace, derived state, counts, and task ids. No capability required (twin of GET /workflows/:id)."
    )]
    pub async fn workflow_get(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        let run = self
            .store()
            .get_run(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        let tasks = self
            .store()
            .list_run_tasks(&args.id)
            .await
            .map_err(McpError::from)?;
        json(workflow_summary(&run, &tasks))
    }

    /// `workflow.start` — **gated** (`Claim`). Activate a workflow: stamp
    /// `started_at` and promote its eligible root tasks to `ready`. The twin of
    /// `POST /workflows/:id/start`. The default `Author` token lacks `Claim`, so
    /// this is the create≠run line — it refuses (403) and the agent hands back for
    /// the operator to press Start (design §6.1).
    #[tool(
        name = "workflow.start",
        description = "GATED (Claim): activate a workflow and promote its eligible root tasks to ready (twin of POST /workflows/:id/start). The default Author token lacks Claim, so this refuses with 403 — the create-is-not-run line."
    )]
    pub async fn workflow_start(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        let session = self.authorize(authorization_header(&parts), Capability::Claim)?;

        self.store()
            .get_run(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;

        let now = self.store().now();
        self.store()
            .mark_run_started(&args.id, &now)
            .await
            .map_err(McpError::from)?;

        let tasks = self
            .store()
            .list_run_tasks(&args.id)
            .await
            .map_err(McpError::from)?;
        let status_by_id: std::collections::HashMap<&str, Status> =
            tasks.iter().map(|t| (t.id.as_str(), t.status)).collect();

        let mut promoted = Vec::new();
        for task in &tasks {
            if task.status != Status::Pending {
                continue;
            }
            // Eligible root: every dependency is `done`.
            let ready = task
                .deps
                .iter()
                .all(|d| status_by_id.get(d.as_str()) == Some(&Status::Done));
            if !ready {
                continue;
            }
            match self
                .store()
                .transition(&task.id, Transition::Ready, session.actor())
                .await
            {
                Ok(_) => promoted.push(task.id.clone()),
                Err(e) => tracing::warn!(task = %task.id, "start: promote failed: {e}"),
            }
        }
        json(json!({ "promoted": promoted }))
    }

    /// `workflow.stop` — **gated** (`Block`). Pause a workflow: lifecycle →
    /// `stopped`, kill live agents, reclaim `running`/`gating` tasks to `ready`
    /// keeping their work. The twin of `POST /workflows/:id/stop`; refuses (403) for
    /// the default `Author` token.
    #[tool(
        name = "workflow.stop",
        description = "GATED (Block): pause a workflow — lifecycle to stopped, kill live agents, reclaim in-flight tasks to ready keeping their work (twin of POST /workflows/:id/stop). Refuses with 403 for an Author token."
    )]
    pub async fn workflow_stop(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        let session = self.authorize(authorization_header(&parts), Capability::Block)?;

        self.store()
            .get_run(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;

        // Pause first so the scheduler stops promoting/claiming immediately.
        let run = self.store().stop_run(&args.id).await.map_err(McpError::from)?;

        let tasks = self
            .store()
            .list_run_tasks(&args.id)
            .await
            .map_err(McpError::from)?;
        for task in &tasks {
            if !matches!(task.status, Status::Running | Status::Gating) {
                continue;
            }
            if let Err(e) = lazybones_engine::cancel_agent(&task.id).await {
                tracing::warn!(task = %task.id, "stop: hcom kill failed (continuing): {e}");
            }
            if let Err(e) = self
                .store()
                .transition(&task.id, Transition::Reclaim, session.actor())
                .await
            {
                tracing::warn!(task = %task.id, "stop: reclaim failed: {e}");
            }
        }

        let tasks = self
            .store()
            .list_run_tasks(&args.id)
            .await
            .map_err(McpError::from)?;
        json(workflow_summary(&run, &tasks))
    }

    /// `workflow.resume` — **gated** (`Block`). Un-pause a workflow: lifecycle →
    /// `active` and reset only its `blocked` tasks to `pending`, leaving everything
    /// else as-is. The twin of `POST /workflows/:id/resume`; refuses (403) for an
    /// `Author` token.
    #[tool(
        name = "workflow.resume",
        description = "GATED (Block): un-pause a workflow — lifecycle to active and reset only its blocked tasks to pending (twin of POST /workflows/:id/resume). Refuses with 403 for an Author token."
    )]
    pub async fn workflow_resume(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        let session = self.authorize(authorization_header(&parts), Capability::Block)?;

        self.store()
            .get_run(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;

        let run = self
            .store()
            .resume_run(&args.id)
            .await
            .map_err(McpError::from)?;

        let tasks = self
            .store()
            .list_run_tasks(&args.id)
            .await
            .map_err(McpError::from)?;
        for task in &tasks {
            if task.status != Status::Blocked {
                continue;
            }
            if let Err(e) = self.store().reset(&task.id, session.actor()).await {
                tracing::warn!(task = %task.id, "resume: reset failed: {e}");
            }
        }

        let tasks = self
            .store()
            .list_run_tasks(&args.id)
            .await
            .map_err(McpError::from)?;
        json(workflow_summary(&run, &tasks))
    }

    /// `workflow.restart` — **gated** (`Block`). Reset a workflow's tasks to
    /// `pending` (clearing claim/worktree/commit) so it can be started fresh; `soft`
    /// keeps `done` tasks + worktrees. The twin of `POST /workflows/:id/restart`;
    /// refuses (403) for an `Author` token. Does **not** re-promote — the operator
    /// presses Start.
    #[tool(
        name = "workflow.restart",
        description = "GATED (Block): reset a workflow's tasks to pending so it can be started fresh; soft keeps done tasks and worktrees (twin of POST /workflows/:id/restart). Refuses with 403 for an Author token."
    )]
    pub async fn workflow_restart(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<WorkflowRestartArgs>,
    ) -> McpResult<Json<Value>> {
        let session = self.authorize(authorization_header(&parts), Capability::Block)?;
        // Default is a full hard reset; `soft` is the resume-style escape hatch.
        let hard = !args.soft;

        let run = self
            .store()
            .get_run(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        let repo = std::path::Path::new(&run.workspace.repo);
        let remote = std::env::var("LAZYBONES_REMOTE").unwrap_or_else(|_| "origin".to_owned());

        // `Shared` mode puts many tasks on ONE branch; dedupe by branch to delete it
        // once, not once per task.
        let mut wiped_branches: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        let tasks = self
            .store()
            .list_run_tasks(&args.id)
            .await
            .map_err(McpError::from)?;
        for task in &tasks {
            if task.status == Status::Done && !hard {
                continue;
            }
            if task.status == Status::Pending {
                continue;
            }
            if matches!(task.status, Status::Running | Status::Gating)
                && let Err(e) = lazybones_engine::cancel_agent(&task.id).await
            {
                tracing::warn!(task = %task.id, "restart: hcom kill failed (continuing): {e}");
            }
            if hard && let Some(path) = &task.worktree {
                let branch = task.branch.as_deref();
                let fresh_branch = branch.is_some_and(|b| wiped_branches.insert(b.to_owned()));
                let branch_arg = if fresh_branch { branch } else { None };
                if let Err(e) =
                    lazybones_engine::remove_worktree(repo, path, branch_arg, Some(&remote)).await
                {
                    tracing::warn!(task = %task.id, "restart: worktree remove failed (continuing): {e}");
                }
            }
            if let Err(e) = self.store().reset(&task.id, session.actor()).await {
                tracing::warn!(task = %task.id, "restart: reset failed: {e}");
            }
        }

        // Deterministic final sweep for `Shared` mode (one tree/branch keyed by the
        // run id that the per-task loop may never have touched). Idempotent.
        if hard && run.workspace.worktree_mode == lazybones_store::WorktreeMode::Shared {
            let prefix = run
                .workspace
                .branch_prefix
                .clone()
                .or_else(|| std::env::var("LAZYBONES_BRANCH_PREFIX").ok())
                .unwrap_or_else(|| "lazy/".to_owned());
            let root = std::env::var("LAZYBONES_WORKTREE_ROOT")
                .unwrap_or_else(|_| ".lazy/wt".to_owned());
            let branch = format!("{prefix}{}", args.id);
            if !wiped_branches.contains(&branch) {
                let path = repo.join(&root).join(&args.id);
                if let Err(e) = lazybones_engine::remove_worktree(
                    repo,
                    &path.to_string_lossy(),
                    Some(&branch),
                    Some(&remote),
                )
                .await
                {
                    tracing::warn!(branch = %branch, "restart: shared-branch sweep failed (continuing): {e}");
                }
            }
        }

        // Un-activate the run (clear `started_at`) so it does not auto-re-run on the
        // next tick; the next `start` re-stamps it. Best-effort.
        let run = match self.store().clear_run_started(&args.id).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(run = %args.id, "restart: clear started_at failed (continuing): {e}");
                run
            }
        };

        let tasks = self
            .store()
            .list_run_tasks(&args.id)
            .await
            .map_err(McpError::from)?;
        json(workflow_summary(&run, &tasks))
    }

    /// `task.create` — author a standalone task (`pending`) and wire its dependency
    /// edges. The twin of `POST /tasks`: requires `Author`. Conflicts if the id is
    /// taken. The task groups under the server's run label ([`McpServer::run_label`]).
    #[tool(
        name = "task.create",
        description = "Author a standalone task (status pending) and wire its dependency edges. Requires the Author capability (twin of POST /tasks)."
    )]
    pub async fn task_create(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<TaskCreateArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;
        let mut task = Task::seed(
            &args.id,
            self.run_label(),
            &args.title,
            &args.spec,
            args.deps.clone(),
            args.owns.clone(),
            args.tool.clone(),
        );
        task.worktree_mode = worktree_mode_or_default(args.worktree_mode);
        let created = self.store().create_task(&task).await.map_err(McpError::from)?;
        for dep in &args.deps {
            self.store()
                .relate_dep(&args.id, dep)
                .await
                .map_err(McpError::from)?;
        }
        json(created)
    }

    /// `task.update` — overwrite a task's authored fields and reconcile its deps
    /// (lifecycle preserved). The twin of `PATCH /tasks/:id`: requires `Author`.
    /// `404` if the task is unknown.
    #[tool(
        name = "task.update",
        description = "Overwrite a task's authored fields and reconcile its dependency edges (lifecycle preserved). Requires the Author capability (twin of PATCH /tasks/:id)."
    )]
    pub async fn task_update(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<TaskUpdateArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;
        let old = self
            .store()
            .get_task(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        let updated = self
            .store()
            .update_task(
                &args.id,
                TaskEdit {
                    title: args.title.clone(),
                    spec: args.spec.clone(),
                    deps: args.deps.clone(),
                    owns: args.owns.clone(),
                    tool: args.tool.clone(),
                    model: args.model.clone(),
                    effort: args.effort.clone(),
                    worktree_mode: worktree_mode_or_default(args.worktree_mode.clone()),
                    // Tri-state folder-trust + the Block-guarded retry/issue config
                    // are not part of this Author-guarded re-authoring (matching the
                    // REST route, which leaves them to their own routes).
                    auto_trust_agent_folder: None,
                    auto_retry: None,
                    max_retries: None,
                    issue_close_on_done: None,
                },
            )
            .await
            .map_err(McpError::from)?;
        for dep in &old.deps {
            if !args.deps.contains(dep) {
                self.store()
                    .unrelate_dep(&args.id, dep)
                    .await
                    .map_err(McpError::from)?;
            }
        }
        for dep in &args.deps {
            if !old.deps.contains(dep) {
                self.store()
                    .relate_dep(&args.id, dep)
                    .await
                    .map_err(McpError::from)?;
            }
        }
        json(updated)
    }

    /// `task.list` — list tasks, optionally narrowed by `status`. An open read, the
    /// twin of `GET /tasks?status=`. An unknown status yields an empty list.
    #[tool(
        name = "task.list",
        description = "List tasks, optionally narrowed by status. No capability required (twin of GET /tasks?status=). An unknown status yields an empty list."
    )]
    pub async fn task_list(
        &self,
        Parameters(args): Parameters<crate::args::TaskListArgs>,
    ) -> McpResult<Json<Value>> {
        let tasks = match args.status.as_deref() {
            None => self.store().list_tasks(None).await.map_err(McpError::from)?,
            Some(raw) => match parse_status(raw) {
                Some(status) => self
                    .store()
                    .list_tasks(Some(status))
                    .await
                    .map_err(McpError::from)?,
                None => Vec::new(),
            },
        };
        json(tasks)
    }

    /// `task.get` — read one task (spec, status, deps, claim state). An open read,
    /// the twin of `GET /tasks/:id`. `404` if unknown.
    #[tool(
        name = "task.get",
        description = "Read one task: spec text, status, deps, claim state. No capability required (twin of GET /tasks/:id)."
    )]
    pub async fn task_get(&self, Parameters(args): Parameters<IdArgs>) -> McpResult<Json<Value>> {
        let task = self
            .store()
            .get_task(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        json(task)
    }

    /// `task.retry` — **gated** (`Block`). Revive ONE blocked task: guided (with a
    /// `strategy`, revived in its kept worktree) or clean (kill agent, optionally
    /// remove the tree, reset to `pending`). The twin of `POST /tasks/:id/retry`;
    /// refuses (403) for an `Author` token.
    #[tool(
        name = "task.retry",
        description = "GATED (Block): revive one blocked task — guided (with a strategy, in its kept worktree) or clean (reset to pending) (twin of POST /tasks/:id/retry). Refuses with 403 for an Author token."
    )]
    pub async fn task_retry(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<TaskRetryArgs>,
    ) -> McpResult<Json<Value>> {
        let session = self.authorize(authorization_header(&parts), Capability::Block)?;

        let task = self
            .store()
            .get_task(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;

        if task.status != Status::Blocked {
            return Err(McpError::conflict(format!(
                "task `{}` is `{}`, not blocked; only a blocked task can be retried",
                args.id,
                task.status.as_str()
            ))
            .into());
        }

        // Guided retry: revive in the kept worktree with the strategy's guidance.
        if let Some(strategy) = RetryStrategy::parse(args.strategy.as_deref()) {
            let reason = task.reason.as_deref().unwrap_or("(no reason recorded)");
            let guidance = strategy.guidance(reason);
            let revived = self
                .store()
                .revive_with_guidance(&args.id, &guidance, session.actor(), false)
                .await
                .map_err(McpError::from)?;
            return json(revived);
        }

        // Clean retry: kill any live agent (best-effort), optionally tear down the
        // worktree, then reset to a fresh `pending`.
        if let Err(e) = lazybones_engine::cancel_agent(&args.id).await {
            tracing::warn!(task = %args.id, "retry: hcom kill failed (continuing): {e}");
        }
        if args.remove_worktrees
            && let Some(path) = &task.worktree
        {
            let run_key = task.run_id.as_deref().unwrap_or(&task.run);
            let run = self
                .store()
                .get_run(run_key)
                .await
                .map_err(McpError::from)?
                .ok_or(McpError::NotFound)?;
            let remote = std::env::var("LAZYBONES_REMOTE").unwrap_or_else(|_| "origin".to_owned());
            if let Err(e) = lazybones_engine::remove_worktree(
                std::path::Path::new(&run.workspace.repo),
                path,
                task.branch.as_deref(),
                Some(&remote),
            )
            .await
            {
                tracing::warn!(task = %args.id, "retry: worktree remove failed (continuing): {e}");
            }
        }

        let reset = self
            .store()
            .reset(&args.id, session.actor())
            .await
            .map_err(McpError::from)?;
        json(reset)
    }

    /// `task.auto_retry` — **gated** (`Block`). Set (or clear) a task's hands-off
    /// retry policy — durable config the scheduler consults on a block. The twin of
    /// `PUT /tasks/:id/auto-retry`; refuses (403) for an `Author` token.
    #[tool(
        name = "task.auto_retry",
        description = "GATED (Block): set or clear a task's hands-off auto-retry policy — durable config consulted on a block (twin of PUT /tasks/:id/auto-retry). Refuses with 403 for an Author token."
    )]
    pub async fn task_auto_retry(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<TaskAutoRetryArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Block)?;

        self.store()
            .get_task(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;

        let strategy = RetryStrategy::parse(args.strategy.as_deref());
        let task = self
            .store()
            .set_retry_policy(&args.id, strategy, args.max_retries)
            .await
            .map_err(McpError::from)?;
        json(task)
    }

    /// `task.cancel` — **gated** (`Block`). Kill the task's live agent then block it
    /// with a reason. The twin of `POST /tasks/:id/cancel`; refuses (403) for an
    /// `Author` token.
    #[tool(
        name = "task.cancel",
        description = "GATED (Block): kill the task's live agent then block it with a reason (twin of POST /tasks/:id/cancel). Refuses with 403 for an Author token."
    )]
    pub async fn task_cancel(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<TaskCancelArgs>,
    ) -> McpResult<Json<Value>> {
        let session = self.authorize(authorization_header(&parts), Capability::Block)?;

        if let Err(e) = lazybones_engine::cancel_agent(&args.id).await {
            tracing::warn!(task = %args.id, "cancel: hcom kill failed (continuing to block): {e}");
        }
        let reason = args
            .reason
            .filter(|r| !r.trim().is_empty())
            .unwrap_or_else(|| "cancelled by operator".to_owned());
        let task = self
            .store()
            .transition(&args.id, Transition::Block { reason }, session.actor())
            .await
            .map_err(McpError::from)?;
        json(task)
    }

    /// `template.create` — author a reusable task template. The twin of
    /// `POST /templates`: requires `Author`. Conflicts if the id is taken.
    #[tool(
        name = "template.create",
        description = "Author a reusable task template. Requires the Author capability (twin of POST /templates)."
    )]
    pub async fn template_create(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<TemplateArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;
        let template = Template::new(
            &args.id,
            &args.title,
            &args.description,
            &args.spec_template,
            args.default_tool.clone(),
            args.default_model.clone(),
            args.default_effort.clone(),
            worktree_mode_override(args.default_worktree_mode.clone()),
            self.store().now(),
        );
        let created = self
            .store()
            .create_template(&template)
            .await
            .map_err(McpError::from)?;
        json(created)
    }

    /// `template.update` — overwrite an existing template (`created_at` preserved).
    /// The twin of `PUT /templates/:id`: requires `Author`. `404` if unknown.
    #[tool(
        name = "template.update",
        description = "Overwrite an existing task template wholesale (created_at preserved). Requires the Author capability (twin of PUT /templates/:id)."
    )]
    pub async fn template_update(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<TemplateArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;
        let template = Template::new(
            &args.id,
            &args.title,
            &args.description,
            &args.spec_template,
            args.default_tool.clone(),
            args.default_model.clone(),
            args.default_effort.clone(),
            worktree_mode_override(args.default_worktree_mode.clone()),
            self.store().now(),
        );
        let updated = self
            .store()
            .update_template(&template)
            .await
            .map_err(McpError::from)?;
        json(updated)
    }

    /// `template.list` — every task template. An open read, twin of `GET /templates`.
    #[tool(
        name = "template.list",
        description = "List every reusable task template. No capability required (twin of GET /templates)."
    )]
    pub async fn template_list(&self) -> McpResult<Json<Value>> {
        let templates = self.store().list_templates().await.map_err(McpError::from)?;
        json(templates)
    }

    /// `template.get` — one task template. An open read, twin of `GET /templates/:id`.
    /// `404` if unknown.
    #[tool(
        name = "template.get",
        description = "Fetch one reusable task template. No capability required (twin of GET /templates/:id)."
    )]
    pub async fn template_get(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        let template = self
            .store()
            .get_template(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        json(template)
    }

    /// `template.delete` — remove a template. The twin of `DELETE /templates/:id`:
    /// requires `Author` (it is authoring config, not a run-lifecycle act). Returns
    /// whether it existed.
    #[tool(
        name = "template.delete",
        description = "Remove a reusable task template. Requires the Author capability (twin of DELETE /templates/:id). Returns whether it existed."
    )]
    pub async fn template_delete(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<IdArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;
        let existed = self
            .store()
            .delete_template(&args.id)
            .await
            .map_err(McpError::from)?;
        json(json!({ "deleted": existed }))
    }

    /// `skill.create` — author a reusable block of agent instructions. The twin of
    /// `POST /skills`: requires `Author`. Conflicts if the id is taken. (The optional
    /// structured `action` stays a REST concern; MCP authors the markdown body.)
    #[tool(
        name = "skill.create",
        description = "Author a reusable block of agent instructions (markdown runbook). Requires the Author capability (twin of POST /skills)."
    )]
    pub async fn skill_create(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<SkillArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;
        let skill = Skill::new(
            &args.id,
            &args.title,
            &args.description,
            &args.body,
            self.store().now(),
        );
        let created = self.store().create_skill(&skill).await.map_err(McpError::from)?;
        json(created)
    }

    /// `skill.update` — overwrite an existing skill (`created_at` preserved). The
    /// twin of `PUT /skills/:id`: requires `Author`. `404` if unknown.
    #[tool(
        name = "skill.update",
        description = "Overwrite an existing agent-instruction skill wholesale (created_at preserved). Requires the Author capability (twin of PUT /skills/:id)."
    )]
    pub async fn skill_update(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<SkillArgs>,
    ) -> McpResult<Json<Value>> {
        self.authorize(authorization_header(&parts), Capability::Author)?;
        let skill = Skill::new(
            &args.id,
            &args.title,
            &args.description,
            &args.body,
            self.store().now(),
        );
        let updated = self.store().update_skill(&skill).await.map_err(McpError::from)?;
        json(updated)
    }

    /// `skill.list` — every skill. An open read, twin of `GET /skills`.
    #[tool(
        name = "skill.list",
        description = "List every reusable agent-instruction skill. No capability required (twin of GET /skills)."
    )]
    pub async fn skill_list(&self) -> McpResult<Json<Value>> {
        let skills = self.store().list_skills().await.map_err(McpError::from)?;
        json(skills)
    }

    /// `skill.get` — one skill. An open read, twin of `GET /skills/:id`. `404` if
    /// unknown.
    #[tool(
        name = "skill.get",
        description = "Fetch one reusable agent-instruction skill. No capability required (twin of GET /skills/:id)."
    )]
    pub async fn skill_get(&self, Parameters(args): Parameters<IdArgs>) -> McpResult<Json<Value>> {
        let skill = self
            .store()
            .get_skill(&args.id)
            .await
            .map_err(McpError::from)?
            .ok_or(McpError::NotFound)?;
        json(skill)
    }

    /// `follow_up.file` — the agent's "needs a human" escape hatch. Files a durable
    /// follow-up against a run, idempotent on `(run, dedup_key)`. The twin of
    /// `POST /follow-ups`, but design §6.1 maps it to **`read`** rather than the
    /// route's `Block`: an authoring agent must be able to flag for a human, so this
    /// requires only an authenticated session with `Read` (which every token holds)
    /// — not the operator `Block` grant. Still refuses an unauthenticated call.
    #[tool(
        name = "follow_up.file",
        description = "File a durable follow-up against a run — the agent's 'needs a human' escape hatch, idempotent on (run, dedup_key). Requires only an authenticated session (Read), so an authoring agent can flag for a human."
    )]
    pub async fn follow_up_file(
        &self,
        Extension(parts): Extension<http::request::Parts>,
        Parameters(args): Parameters<FollowUpFileArgs>,
    ) -> McpResult<Json<Value>> {
        let session = self.authorize(authorization_header(&parts), Capability::Read)?;
        let dedup_key = args.dedup_key.unwrap_or_else(|| args.title.clone());
        let entry = NewFollowUpEntry {
            run: args.run,
            task: args.task,
            dedup_key,
            kind: args.kind.unwrap_or_else(|| "note".to_owned()),
            title: args.title,
            detail: args.detail,
            actor: session.actor().to_owned(),
        };
        let filed = self.store().file_follow_up(entry).await.map_err(McpError::from)?;
        json(filed)
    }
}

/// Map a query-string status to a [`Status`]; `None` for unknown values — the twin
/// of the REST list route's parser, so `task.list` accepts the same strings.
fn parse_status(s: &str) -> Option<Status> {
    match s {
        "pending" => Some(Status::Pending),
        "ready" => Some(Status::Ready),
        "running" => Some(Status::Running),
        "gating" => Some(Status::Gating),
        "done" => Some(Status::Done),
        "blocked" => Some(Status::Blocked),
        _ => None,
    }
}
