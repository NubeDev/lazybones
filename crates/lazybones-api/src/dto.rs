//! Wire types for the REST surface (request bodies + the task projection).
//!
//! The domain [`Task`](lazybones_store::Task) already derives serde, so it *is*
//! the task DTO — these are the small request bodies the mutating routes accept.

use serde::{Deserialize, Serialize};

use lazybones_store::{Run, Task, WorktreeMode};

/// `POST /tasks/:id/claim` body: where the agent will work.
#[derive(Debug, Deserialize)]
pub struct ClaimBody {
    /// hcom session id that took the task.
    pub session: String,
    /// The git worktree path the agent edits.
    pub worktree: String,
    /// The branch the agent commits to.
    pub branch: String,
    /// The bearer token to mint for this agent session.
    pub token: String,
}

/// `POST /tasks/:id/heartbeat` body: optional liveness payload.
///
/// Backward-compatible: an empty body still pings. A `note` rides along as an
/// agent progress message, broadcast on the live feed (`activity` SSE event) so
/// the user can see the agent working.
#[derive(Debug, Default, Deserialize)]
pub struct HeartbeatBody {
    /// Optional progress message ("running cargo test…").
    #[serde(default)]
    pub note: Option<String>,
}

/// `POST /tasks/:id/activity` body: a free-form agent progress message.
#[derive(Debug, Deserialize)]
pub struct ActivityBody {
    /// The human-readable progress message to broadcast on the live feed.
    pub message: String,
}

/// `POST /tasks/:id/done` body: the commit the agent pushed.
#[derive(Debug, Deserialize)]
pub struct DoneBody {
    /// The commit sha that landed on the task branch.
    pub commit: String,
}

/// `POST /tasks/:id/block` body: why it could not finish.
#[derive(Debug, Deserialize)]
pub struct BlockBody {
    /// Human-readable reason, recorded on the task and in the run log.
    pub reason: String,
}

/// `POST /tasks/:id/cancel` body: an optional reason (defaults when omitted).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CancelBody {
    /// Why the task was cancelled; a blank/absent value records a default.
    pub reason: Option<String>,
}

/// `POST /tasks` body: a new task to author (status starts `Pending`).
#[derive(Debug, Deserialize)]
pub struct CreateTaskBody {
    /// The unique task id; `409` if it is already taken.
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// The spec text (the agent's brief).
    pub spec: String,
    /// Ids of tasks this one depends on; wired as graph edges.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Paths/areas this task owns, for conflict avoidance.
    #[serde(default)]
    pub owns: Vec<String>,
    /// The agent tool that should run this task, if pinned.
    #[serde(default)]
    pub tool: Option<String>,
    /// How the loop should provision the worktree on claim; defaults to `new`.
    #[serde(default)]
    pub worktree_mode: WorktreeMode,
}

/// `PATCH /tasks/:id` body: overwrite the authored fields of a task.
#[derive(Debug, Deserialize)]
pub struct UpdateTaskBody {
    /// New title.
    pub title: String,
    /// New spec text.
    pub spec: String,
    /// New dependency ids; edges are reconciled against the old set.
    #[serde(default)]
    pub deps: Vec<String>,
    /// New owned paths/areas.
    #[serde(default)]
    pub owns: Vec<String>,
    /// New pinned agent tool, if any.
    #[serde(default)]
    pub tool: Option<String>,
    /// New worktree provisioning intent; defaults to `new`.
    #[serde(default)]
    pub worktree_mode: WorktreeMode,
}

/// `POST /templates` body: a reusable task recipe to author.
#[derive(Debug, Deserialize)]
pub struct CreateTemplateBody {
    /// Unique template id; `409` if it is already taken.
    pub id: String,
    /// Human title.
    pub title: String,
    /// Optional longer description shown in the picker.
    #[serde(default)]
    pub description: String,
    /// Starting spec text for tasks instantiated from this template.
    pub spec_template: String,
    /// Agent tool inherited by the task unless overridden.
    #[serde(default)]
    pub default_tool: Option<String>,
    /// Model inherited by the task unless overridden; omitted inherits.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Effort inherited by the task unless overridden; omitted inherits.
    #[serde(default)]
    pub default_effort: Option<String>,
    /// Rarely-set worktree mode intrinsic to the recipe; usually omitted.
    #[serde(default)]
    pub default_worktree_mode: Option<WorktreeMode>,
}

/// The workspace sub-object of a `POST /workflows` body.
#[derive(Debug, Deserialize)]
pub struct WorkspaceBody {
    /// Absolute path to the target git repo.
    pub repo: String,
    /// Base branch override; omitted inherits the global `EngineConfig`.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// Branch-prefix override; omitted inherits the global `EngineConfig`.
    #[serde(default)]
    pub branch_prefix: Option<String>,
    /// Default git mode for this workflow's tasks.
    #[serde(default)]
    pub worktree_mode: WorktreeMode,
    /// Default agent tool for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub tool: Option<String>,
    /// Default model for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub model: Option<String>,
    /// Default effort for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub effort: Option<String>,
}

/// `POST /workflows` body: a new workflow bound to a workspace.
#[derive(Debug, Deserialize)]
pub struct CreateWorkflowBody {
    /// Unique workflow id; `409` if it is already taken.
    pub id: String,
    /// Human title.
    pub title: String,
    /// The repo + inherited git config.
    pub workspace: WorkspaceBody,
}

/// `POST /workflows/:id/tasks` body: add a task to the workflow.
///
/// When `from_template` is set, the task is instantiated from that template
/// (spec/tool/default mode); the explicit fields below then refine it.
#[derive(Debug, Deserialize)]
pub struct AddWorkflowTaskBody {
    /// The unique task id; `409` if it is already taken.
    pub id: String,
    /// Human title.
    pub title: String,
    /// Spec text. Ignored when `from_template` is set (the template supplies it).
    #[serde(default)]
    pub spec: String,
    /// Instantiate from this template id, if set.
    #[serde(default)]
    pub from_template: Option<String>,
    /// Dependency ids within the workflow; wired as graph edges.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Paths/areas this task owns.
    #[serde(default)]
    pub owns: Vec<String>,
    /// Per-task agent tool override.
    #[serde(default)]
    pub tool: Option<String>,
    /// Per-task model id forwarded to the agent CLI (one of the agent catalog's
    /// `models`); omitted lets the CLI use its default.
    #[serde(default)]
    pub model: Option<String>,
    /// Per-task effort level forwarded to the agent CLI (one of the agent
    /// catalog's `efforts`); omitted lets the CLI use its default.
    #[serde(default)]
    pub effort: Option<String>,
    /// Workflow-only worktree-mode override; omitted inherits the workspace mode.
    #[serde(default)]
    pub worktree_mode_override: Option<WorktreeMode>,
    /// For `reuse` mode: the task id whose worktree to reuse (cross-workflow).
    #[serde(default)]
    pub reuse_from: Option<String>,
}

/// `GET /workflows` list item: a run with its derived state and task counts.
#[derive(Debug, Serialize)]
pub struct WorkflowSummary {
    /// The stored run (workspace, lifecycle, timestamps).
    #[serde(flatten)]
    pub run: Run,
    /// The derived, never-stored state (`draft|ready|running|...`).
    pub state: &'static str,
    /// Total tasks linked to this workflow.
    pub task_count: usize,
    /// How many of those tasks are `done`.
    pub done_count: usize,
}

impl WorkflowSummary {
    /// Build a summary from a run and its tasks (derives state + counts).
    #[must_use]
    pub fn new(run: Run, tasks: &[Task]) -> Self {
        let state = lazybones_store::derived_state(run.lifecycle, tasks).as_str();
        let done_count = tasks
            .iter()
            .filter(|t| t.status == lazybones_store::Status::Done)
            .count();
        Self {
            run,
            state,
            task_count: tasks.len(),
            done_count,
        }
    }
}

/// `GET /workflows/:id` detail: the summary plus the generated task ids.
#[derive(Debug, Serialize)]
pub struct WorkflowDetail {
    /// The run + derived state + counts.
    #[serde(flatten)]
    pub summary: WorkflowSummary,
    /// The ids of the tasks linked to this workflow.
    pub task_ids: Vec<String>,
}

/// `PUT /secrets/:tool` body: the credential to seal for an agent CLI.
#[derive(Debug, Deserialize)]
pub struct SecretBody {
    /// The environment variable the agent CLI reads (e.g. `ANTHROPIC_API_KEY`).
    pub env_var: String,
    /// The secret value (API key / token). Sealed at rest; never read back.
    pub value: String,
}

/// `POST /agent-catalog` body: author a new agent catalog entry.
#[derive(Debug, Deserialize)]
pub struct CreateAgentBody {
    /// The tool id — must match the hcom tool key (e.g. `claude`); `409` if taken.
    pub id: String,
    /// Human label for the UI.
    pub label: String,
    /// The env var the CLI reads its credential from.
    pub env_var: String,
    /// How to obtain a credential / log in (shown as a hint).
    #[serde(default)]
    pub login_hint: String,
    /// Selectable model ids, most-preferred first; empty = no model picker.
    #[serde(default)]
    pub models: Vec<String>,
    /// Default model when a task names none.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Selectable effort levels; empty = no effort picker.
    #[serde(default)]
    pub efforts: Vec<String>,
    /// Default effort when a task names none.
    #[serde(default)]
    pub default_effort: Option<String>,
}

/// `PATCH /agent-catalog/:id` body: the authored fields to overwrite. `id` and
/// `created_at` are preserved; `updated_at` is bumped server-side.
#[derive(Debug, Deserialize)]
pub struct UpdateAgentBody {
    /// New human label.
    pub label: String,
    /// New credential env var.
    pub env_var: String,
    /// New login hint.
    #[serde(default)]
    pub login_hint: String,
    /// New model menu.
    #[serde(default)]
    pub models: Vec<String>,
    /// New default model.
    #[serde(default)]
    pub default_model: Option<String>,
    /// New effort menu.
    #[serde(default)]
    pub efforts: Vec<String>,
    /// New default effort.
    #[serde(default)]
    pub default_effort: Option<String>,
}
