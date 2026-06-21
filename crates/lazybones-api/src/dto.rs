//! Wire types for the REST surface (request bodies + the task projection).
//!
//! The domain [`Task`](lazybones_store::Task) already derives serde, so it *is*
//! the task DTO — these are the small request bodies the mutating routes accept.

use serde::{Deserialize, Serialize};

use lazybones_store::{MergeMode, Run, Task, WorktreeMode};

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

/// `POST /tasks/:id/chat` body: a message from the operator to the task's agent.
#[derive(Debug, Deserialize)]
pub struct ChatBody {
    /// The message text to post on the task's hcom thread.
    pub text: String,
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

/// `POST /follow-ups` body: an agent (or operator) flags something for human
/// attention. The run is taken from the path-free body so an agent can file
/// against its own run; `kind` is a coarse class the UI groups by.
#[derive(Debug, Deserialize)]
pub struct FollowUpBody {
    /// The run this follow-up belongs to (the workflow `run_id`).
    pub run: String,
    /// The task it concerns, if any.
    #[serde(default)]
    pub task: Option<String>,
    /// Coarse class: `consent` | `credential` | `spawn` | `gate` | `note`.
    /// Defaults to `note` when omitted.
    #[serde(default)]
    pub kind: Option<String>,
    /// One-line summary.
    pub title: String,
    /// Full reason + suggested fix (markdown).
    pub detail: String,
    /// Optional idempotency key — re-filing the same `(run, dedup_key)` bumps the
    /// existing follow-up instead of creating a duplicate. Defaults to the title
    /// when omitted, so repeated identical titles coalesce.
    #[serde(default)]
    pub dedup_key: Option<String>,
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

/// `PUT /templates/:id` body: the new state of an existing template. The id
/// comes from the path; every other field is overwritten wholesale.
#[derive(Debug, Deserialize)]
pub struct UpdateTemplateBody {
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

/// `POST /skills` body: a reusable block of agent instructions to author.
#[derive(Debug, Deserialize)]
pub struct CreateSkillBody {
    /// Unique skill id; `409` if it is already taken.
    pub id: String,
    /// Human title.
    pub title: String,
    /// Optional longer description shown in the picker.
    #[serde(default)]
    pub description: String,
    /// The skill text/instructions an agent follows (markdown).
    #[serde(default)]
    pub body: String,
    /// An optional structured action for deterministic execution (scope §6.1).
    #[serde(default)]
    pub action: Option<lazybones_store::SkillAction>,
}

/// `PUT /skills/:id` body: the new state of an existing skill. The id comes from
/// the path; every other field is overwritten wholesale.
#[derive(Debug, Deserialize)]
pub struct UpdateSkillBody {
    /// Human title.
    pub title: String,
    /// Optional longer description shown in the picker.
    #[serde(default)]
    pub description: String,
    /// The skill text/instructions an agent follows (markdown).
    #[serde(default)]
    pub body: String,
    /// An optional structured action for deterministic execution (scope §6.1).
    #[serde(default)]
    pub action: Option<lazybones_store::SkillAction>,
}

/// `POST /:owner/:id/attachments` body: a polymorphic thing to attach. The owner
/// is fixed by the route (`owner_kind` + the path id); the thing is open.
#[derive(Debug, Deserialize)]
pub struct AttachBody {
    /// The attached thing's kind (e.g. `skill`).
    pub thing_kind: String,
    /// The attached thing's id (its uuid/friendly key, e.g. `code-review-rust`).
    pub thing_id: String,
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
    /// Green-build gate for this workflow's tasks (e.g. `["npm test --prefix backend"]`).
    /// Omitted/`null` inherits the global `EngineConfig.gate`; an explicit empty list
    /// `[]` disables the gate (tasks land on agent DONE with no command run).
    #[serde(default)]
    pub gate: Option<Vec<String>>,
    /// Merge strategy for this workflow's green branches (`fast-forward` | `merge`
    /// | `pr`). Omitted/`null` inherits the global `EngineConfig.merge`, so a repo
    /// wanting strict linear history can pin `fast-forward` while others use `merge`.
    #[serde(default)]
    pub merge: Option<MergeMode>,
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
    /// RFC3339 timestamp the workflow reached a terminal state, **derived** from
    /// its tasks (the latest task `finished_at`/`failed_at`) and never stored —
    /// only set once every task is terminal (all `done`, or some `blocked` with
    /// none still live). `None` while the workflow is still in flight. The UI
    /// derives "time taken" from `run.started_at` → this. (SCOPE.md principle 6:
    /// the DB is truth; a stored rollup would drift.)
    pub finished_at: Option<String>,
    /// RFC3339 timestamp of the workflow's most recent task failure (the latest
    /// `failed_at` across its tasks), or `None` if no task has failed. Distinct
    /// from `finished_at`: a workflow with a blocked task that an operator has
    /// not yet revived has a `failed_at` but no `finished_at` until it settles.
    pub failed_at: Option<String>,
}

impl WorkflowSummary {
    /// Build a summary from a run and its tasks (derives state + counts + timing).
    #[must_use]
    pub fn new(run: Run, tasks: &[Task]) -> Self {
        let state = lazybones_store::derived_state(run.lifecycle, tasks).as_str();
        let done_count = tasks
            .iter()
            .filter(|t| t.status == lazybones_store::Status::Done)
            .count();
        let failed_at = tasks.iter().filter_map(|t| t.failed_at.clone()).max();
        // The workflow has "finished" only once no task is still in flight
        // (pending/ready/running/gating). Its finish instant is then the latest
        // terminal task stamp — the moment the last unit settled.
        let in_flight = tasks.iter().any(|t| {
            matches!(
                t.status,
                lazybones_store::Status::Pending
                    | lazybones_store::Status::Ready
                    | lazybones_store::Status::Running
                    | lazybones_store::Status::Gating
            )
        });
        let finished_at = if tasks.is_empty() || in_flight {
            None
        } else {
            tasks
                .iter()
                .filter_map(|t| t.finished_at.clone().or_else(|| t.failed_at.clone()))
                .max()
        };
        Self {
            run,
            state,
            task_count: tasks.len(),
            done_count,
            finished_at,
            failed_at,
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

/// `PUT /settings/management-agent` body: the single global Lazybones-Agent
/// configuration. `model`/`effort` are validated against the agent catalog;
/// `permission_profile` and `session_mode` parse leniently (unknown ⇒ safe
/// default) in the store layer.
#[derive(Debug, Deserialize)]
pub struct ManagementAgentBody {
    /// FK into the agent catalog, e.g. `"claude"`.
    pub tool: String,
    /// Model ⊆ the tool's catalog entry, or `None` for the CLI default.
    #[serde(default)]
    pub model: Option<String>,
    /// Effort ⊆ the tool's catalog entry, or `None` for the CLI default.
    #[serde(default)]
    pub effort: Option<String>,
    /// `"read_only" | "author"`.
    pub permission_profile: String,
    /// `"per_conversation" | "per_turn"`.
    #[serde(default = "default_session_mode")]
    pub session_mode: String,
    /// Skill ids the agent may use as operating runbooks.
    #[serde(default)]
    pub enabled_skills: Vec<String>,
    /// Extra CLI flags for the tool process.
    #[serde(default)]
    pub permission_flags: Vec<String>,
}

/// Default session mode when the client omits it.
fn default_session_mode() -> String {
    "per_conversation".to_owned()
}

/// `PUT /settings/preferences` body: the single global user-preferences record.
/// Every field is optional; an omitted field clears that preference (reverts to
/// the default — follow-browser timezone, system theme).
#[derive(Debug, Default, Deserialize)]
pub struct PreferencesBody {
    /// IANA timezone name, or `None`/empty to follow the browser.
    #[serde(default)]
    pub timezone: Option<String>,
    /// `"light" | "dark" | "system"`, or `None` for system.
    #[serde(default)]
    pub theme: Option<String>,
}

/// `POST /agent/chat` body: one operator turn for the Lazybones Agent.
#[derive(Debug, Deserialize)]
pub struct AgentChatBody {
    /// The conversation to continue; absent opens a new one.
    #[serde(default)]
    pub conversation: Option<String>,
    /// The operator's message.
    pub text: String,
    /// The page the operator is viewing, as a typed envelope (scope §7). Opaque
    /// JSON here; rendered by the engine into the system prompt.
    #[serde(default)]
    pub page_context: Option<serde_json::Value>,
}

/// `POST /agent/chat` response: the conversation id + the stored operator turn.
/// The agent's reply arrives over the per-conversation SSE stream.
#[derive(Debug, Serialize)]
pub struct AgentChatPosted {
    /// The conversation this turn belongs to (newly minted if none was sent).
    pub conversation: String,
    /// The persisted operator message.
    pub message: lazybones_store::AgentMessage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazybones_store::{Status, WorktreeMode};

    fn run() -> Run {
        Run::new(
            "wf",
            "WF",
            lazybones_store::Workspace {
                repo: "/repo".into(),
                base_branch: None,
                branch_prefix: None,
                worktree_mode: WorktreeMode::New,
                tool: None,
                model: None,
                effort: None,
                gate: None,
                merge: None,
            },
            "2026-01-01T00:00:00Z",
        )
    }

    fn task(id: &str, status: Status) -> Task {
        let mut t = Task::seed(id, "wf", id, "s", vec![], vec![], None);
        t.status = status;
        t
    }

    #[test]
    fn in_flight_workflow_has_no_finish_stamp() {
        let mut a = task("a", Status::Done);
        a.finished_at = Some("2026-01-01T01:00:00Z".into());
        let b = task("b", Status::Running); // still live
        let s = WorkflowSummary::new(run(), &[a, b]);
        assert_eq!(s.finished_at, None);
        assert_eq!(s.failed_at, None);
    }

    #[test]
    fn all_done_finishes_at_latest_task_stamp() {
        let mut a = task("a", Status::Done);
        a.finished_at = Some("2026-01-01T01:00:00Z".into());
        let mut b = task("b", Status::Done);
        b.finished_at = Some("2026-01-01T02:00:00Z".into());
        let s = WorkflowSummary::new(run(), &[a, b]);
        assert_eq!(s.finished_at.as_deref(), Some("2026-01-01T02:00:00Z"));
        assert_eq!(s.failed_at, None);
    }

    #[test]
    fn settled_with_block_reports_finish_and_fail() {
        // Every task terminal (one done, one blocked) → the run has settled. Its
        // finish is the latest terminal stamp; failed_at surfaces the failure.
        let mut a = task("a", Status::Done);
        a.finished_at = Some("2026-01-01T01:00:00Z".into());
        let mut b = task("b", Status::Blocked);
        b.failed_at = Some("2026-01-01T03:00:00Z".into());
        let s = WorkflowSummary::new(run(), &[a, b]);
        assert_eq!(s.finished_at.as_deref(), Some("2026-01-01T03:00:00Z"));
        assert_eq!(s.failed_at.as_deref(), Some("2026-01-01T03:00:00Z"));
    }

    #[test]
    fn blocked_but_revivable_run_still_in_flight_keeps_fail_only() {
        // A blocked task alongside a running one: the run has not settled, so no
        // finish stamp — but the failure is still surfaced.
        let mut a = task("a", Status::Blocked);
        a.failed_at = Some("2026-01-01T03:00:00Z".into());
        let b = task("b", Status::Running);
        let s = WorkflowSummary::new(run(), &[a, b]);
        assert_eq!(s.finished_at, None);
        assert_eq!(s.failed_at.as_deref(), Some("2026-01-01T03:00:00Z"));
    }
}
