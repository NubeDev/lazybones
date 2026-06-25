//! Shared tool-argument DTOs.
//!
//! MCP tool inputs are JSON objects whose JSON-Schema rmcp derives from
//! `schemars`-derived argument structs (the `#[tool]` macro reads the struct in the
//! method signature). The cross-tool input shapes (ids, pagination, common
//! filters) live here so the `tools::*` modules share one definition rather than
//! redeclaring them per verb.
//!
//! `schemars` is re-exported by `rmcp` (`rmcp::schemars`), so the DTOs derive
//! [`JsonSchema`](rmcp::schemars::JsonSchema) without a separate dep. These are the
//! typed twins of the REST request bodies in
//! [`lazybones-api`'s `dto.rs`](../../../crates/lazybones-api/src/dto.rs); enum
//! fields are carried as strings here and parsed through the store's own
//! string<->enum mappers so the two surfaces accept the same wire shape.

use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use lazybones_store::{BrandColors, BrandFonts, MergeMode, Workspace, WorktreeMode};

/// Parse an optional worktree-mode string into a concrete [`WorktreeMode`],
/// falling back to the enum default (`Shared`) when absent — matching the REST
/// DTOs' `#[serde(default)]` on `WorktreeMode`. A present-but-unknown string parses
/// through the store's own [`WorktreeMode::parse`] (its `New` fallback).
#[must_use]
pub fn worktree_mode_or_default(raw: Option<String>) -> WorktreeMode {
    raw.map_or_else(WorktreeMode::default, |m| WorktreeMode::parse(Some(&m)))
}

/// Parse an optional worktree-mode string into `Option<WorktreeMode>` — `None`
/// stays `None` (inherit), a present string parses through [`WorktreeMode::parse`].
/// Used for the override fields the REST DTOs carry as `Option<WorktreeMode>`.
#[must_use]
pub fn worktree_mode_override(raw: Option<String>) -> Option<WorktreeMode> {
    raw.map(|m| WorktreeMode::parse(Some(&m)))
}

/// Address one record by its id — the typed twin of the `:id` path segment shared
/// by the read/get and lifecycle/delete tools (`workflow.get`/`start`/`stop`/…,
/// `task.get`, `template.get`/`delete`, `skill.get`). The enum fields the
/// lifecycle bodies carry live on their own arg structs below.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct IdArgs {
    /// The record id to act on.
    pub id: String,
}

/// Arguments for `workflow.add_task` — the typed twin of `POST /workflows/:id/tasks`
/// ([`AddWorkflowTaskBody`](../../../crates/lazybones-api/src/dto.rs)). When
/// `from_template` is set the spec/tool/default-mode come from that template; the
/// explicit fields then refine the instantiated task.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct WorkflowAddTaskArgs {
    /// The workflow (run) id to add the task to; `404` if unknown.
    pub workflow_id: String,
    /// The unique task id; conflicts if it is taken.
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
    /// Per-task model id forwarded to the agent CLI.
    #[serde(default)]
    pub model: Option<String>,
    /// Per-task effort level forwarded to the agent CLI.
    #[serde(default)]
    pub effort: Option<String>,
    /// Workflow-only worktree-mode override (`new` | `reuse` | `branch` | `shared`);
    /// omitted inherits the workspace mode.
    #[serde(default)]
    pub worktree_mode_override: Option<String>,
    /// For `reuse` mode: the task id whose worktree to reuse (cross-workflow).
    #[serde(default)]
    pub reuse_from: Option<String>,
}

/// Arguments for `task.create` — the typed twin of `POST /tasks`
/// ([`CreateTaskBody`](../../../crates/lazybones-api/src/dto.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TaskCreateArgs {
    /// The unique task id; conflicts if it is taken.
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
    /// How the loop should provision the worktree on claim (`new` | `reuse` |
    /// `branch` | `shared`); omitted defaults to `shared`.
    #[serde(default)]
    pub worktree_mode: Option<String>,
}

/// Arguments for `task.update` — the typed twin of `PATCH /tasks/:id`
/// ([`UpdateTaskBody`](../../../crates/lazybones-api/src/dto.rs)). Overwrites the
/// authored fields and reconciles the dependency edges; lifecycle is preserved.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TaskUpdateArgs {
    /// The task id to edit; `404` if unknown.
    pub id: String,
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
    /// New per-task model id; omitted/`null` inherits the run/global default.
    #[serde(default)]
    pub model: Option<String>,
    /// New per-task effort level; omitted/`null` inherits the run/global default.
    #[serde(default)]
    pub effort: Option<String>,
    /// New worktree provisioning intent (`new` | `reuse` | `branch` | `shared`);
    /// omitted defaults to `shared`.
    #[serde(default)]
    pub worktree_mode: Option<String>,
}

/// Arguments for `template.create` / `template.update` — the typed twin of
/// `POST /templates` / `PUT /templates/:id`
/// ([`CreateTemplateBody`](../../../crates/lazybones-api/src/dto.rs)). On update the
/// `id` names the existing template (`created_at` preserved server-side).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TemplateArgs {
    /// Unique template id; create conflicts if taken, update `404`s if unknown.
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
    /// Rarely-set worktree mode intrinsic to the recipe (`new` | `reuse` | `branch`
    /// | `shared`); usually omitted.
    #[serde(default)]
    pub default_worktree_mode: Option<String>,
}

/// Arguments for `skill.create` / `skill.update` — the typed twin of `POST /skills`
/// / `PUT /skills/:id` ([`CreateSkillBody`](../../../crates/lazybones-api/src/dto.rs)).
/// The optional structured `action` stays a REST-only concern (it is not part of the
/// JSON-schema'd MCP arg surface); MCP authors the markdown runbook.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SkillArgs {
    /// Unique skill id; create conflicts if taken, update `404`s if unknown.
    pub id: String,
    /// Human title.
    pub title: String,
    /// Optional longer description shown in the picker.
    #[serde(default)]
    pub description: String,
    /// The skill text/instructions an agent follows (markdown).
    #[serde(default)]
    pub body: String,
}

/// Arguments for `follow_up.file` — the typed twin of `POST /follow-ups`
/// ([`FollowUpBody`](../../../crates/lazybones-api/src/dto.rs)). The agent's "needs a
/// human" escape hatch; idempotent on `(run, dedup_key)`.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FollowUpFileArgs {
    /// The run (workflow id) this follow-up belongs to.
    pub run: String,
    /// The task it concerns, if any.
    #[serde(default)]
    pub task: Option<String>,
    /// Coarse class: `consent` | `credential` | `spawn` | `gate` | `note`. Defaults
    /// to `note`.
    #[serde(default)]
    pub kind: Option<String>,
    /// One-line summary.
    pub title: String,
    /// Full reason + suggested fix (markdown).
    pub detail: String,
    /// Optional idempotency key; re-filing the same `(run, dedup_key)` bumps the
    /// existing follow-up. Defaults to the title.
    #[serde(default)]
    pub dedup_key: Option<String>,
}

/// Arguments for the gated `workflow.restart` lifecycle tool — the twin of
/// `POST /workflows/:id/restart` ([`RestartBody`](../../../crates/lazybones-api/src/routes/workflows_restart.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct WorkflowRestartArgs {
    /// The workflow id to restart.
    pub id: String,
    /// Soften to a resume-style restart: keep `done` tasks and each task's worktree
    /// + branch. Default `false` (a full hard reset).
    #[serde(default)]
    pub soft: bool,
}

/// Arguments for the gated `task.retry` lifecycle tool — the twin of
/// `POST /tasks/:id/retry` ([`RetryBody`](../../../crates/lazybones-api/src/routes/tasks_retry.rs)).
/// `strategy` is carried as a string (`long_term` | `quick`) and parsed through the
/// store's own [`RetryStrategy::parse`](lazybones_store::RetryStrategy::parse).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TaskRetryArgs {
    /// The blocked task id to revive.
    pub id: String,
    /// Guided-retry fix intent (`long_term` | `quick`). Omitted ⇒ a clean reset.
    #[serde(default)]
    pub strategy: Option<String>,
    /// Clean-reset only: also remove the task's worktree before resetting. Ignored
    /// for a guided retry.
    #[serde(default)]
    pub remove_worktrees: bool,
}

/// Arguments for the gated `task.auto_retry` lifecycle tool — the twin of
/// `PUT /tasks/:id/auto-retry` ([`AutoRetryBody`](../../../crates/lazybones-api/src/routes/tasks_retry_policy.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TaskAutoRetryArgs {
    /// The task id whose policy to set.
    pub id: String,
    /// The hands-off fix intent (`long_term` | `quick`), or omitted to disable.
    #[serde(default)]
    pub strategy: Option<String>,
    /// Cap on hands-off re-attempts; omitted leaves the current cap unchanged.
    #[serde(default)]
    pub max_retries: Option<u32>,
}

/// Arguments for the gated `task.cancel` lifecycle tool — the twin of
/// `POST /tasks/:id/cancel` ([`CancelBody`](../../../crates/lazybones-api/src/dto.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TaskCancelArgs {
    /// The task id to cancel.
    pub id: String,
    /// Why it was cancelled; a blank/absent value records a default.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Arguments for `task.list` — the twin of `GET /tasks?status=`.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TaskListArgs {
    /// Narrow to one lifecycle status (`pending` | `ready` | `running` | `gating` |
    /// `done` | `blocked`); an unknown value yields an empty list.
    #[serde(default)]
    pub status: Option<String>,
}

/// Arguments for `run.history` / `run.follow_ups` / `run.hcom_log` — a run id plus
/// the read filters those supervision routes accept (design §6.4).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RunReadArgs {
    /// The run (workflow id) to read.
    pub run: String,
    /// `run.follow_ups`: restrict to `open` or `resolved`; both when omitted.
    #[serde(default)]
    pub status: Option<String>,
    /// `run.follow_ups` / `run.hcom_log`: restrict to one task.
    #[serde(default)]
    pub task: Option<String>,
    /// `run.hcom_log`: restrict to one event kind (`message` | `status` | `life`).
    #[serde(default)]
    pub kind: Option<String>,
    /// `run.hcom_log`: page boundary — only events with `hcom_id > after`.
    #[serde(default)]
    pub after: Option<i64>,
    /// `run.hcom_log`: page-size cap.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Arguments for `task.hcom_log` — one task's raw agent log, with the page filters
/// the REST route accepts (design §6.4). `task.transcript` reuses [`IdArgs`].
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct TaskHcomLogArgs {
    /// The task id whose agent log to read.
    pub id: String,
    /// Restrict to one event kind (`message` | `status` | `life`).
    #[serde(default)]
    pub kind: Option<String>,
    /// Page boundary: only events with `hcom_id > after`.
    #[serde(default)]
    pub after: Option<i64>,
    /// Page-size cap.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Arguments for `workflow.create` — the typed twin of the REST `POST /workflows`
/// body ([`CreateWorkflowBody`](../../../crates/lazybones-api/src/dto.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct WorkflowCreateArgs {
    /// Unique workflow id; the call conflicts (409-equivalent) if it is taken.
    pub id: String,
    /// Human title.
    pub title: String,
    /// The repo + inherited git/agent config the workflow's tasks default to.
    pub workspace: WorkspaceArgs,
}

/// The workspace sub-object of [`WorkflowCreateArgs`]. Mirrors the REST
/// [`WorkspaceBody`](../../../crates/lazybones-api/src/dto.rs) field-for-field; the
/// two enum fields (`worktree_mode`, `merge`) are strings here, parsed via the
/// store's own mappers so a client sends the same wire form REST accepts.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct WorkspaceArgs {
    /// Absolute path to the target git repo.
    pub repo: String,
    /// Base branch override; omitted inherits the global `EngineConfig`.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// Branch-prefix override; omitted inherits the global `EngineConfig`.
    #[serde(default)]
    pub branch_prefix: Option<String>,
    /// Default git mode (`new` | `reuse` | `branch` | `shared`); omitted defaults to
    /// `shared`, matching the REST DTO.
    #[serde(default)]
    pub worktree_mode: Option<String>,
    /// Names the shared worktree dir + branch (for `new`/`shared` modes), overriding
    /// the id-derived default. Omitted keeps the default behaviour.
    #[serde(default)]
    pub worktree_name: Option<String>,
    /// Default agent tool for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub tool: Option<String>,
    /// Default model for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub model: Option<String>,
    /// Default effort for this workflow's tasks; omitted inherits the global.
    #[serde(default)]
    pub effort: Option<String>,
    /// Green-build gate commands; omitted/`null` inherits the global gate, an
    /// explicit empty list disables it.
    #[serde(default)]
    pub gate: Option<Vec<String>>,
    /// Merge strategy (`fast-forward` | `merge` | `pr`); omitted/`null` inherits the
    /// global.
    #[serde(default)]
    pub merge: Option<String>,
    /// Open a GitHub PR automatically once every task is done. Omitted/`null` = off.
    #[serde(default)]
    pub auto_pr: Option<bool>,
}

impl WorkspaceArgs {
    /// Build the domain [`Workspace`] the store stores, parsing the enum strings
    /// through the store's own mappers — the same translation the REST route does.
    #[must_use]
    pub fn into_workspace(self) -> Workspace {
        Workspace {
            repo: self.repo,
            base_branch: self.base_branch,
            branch_prefix: self.branch_prefix,
            // Absent → `Shared` (the store enum default + the REST DTO's
            // `#[serde(default)]`), not the parse-fallback `New`.
            worktree_mode: self
                .worktree_mode
                .map_or_else(WorktreeMode::default, |m| WorktreeMode::parse(Some(&m))),
            worktree_name: self.worktree_name,
            tool: self.tool,
            model: self.model,
            effort: self.effort,
            gate: self.gate,
            merge: self.merge.map(|m| MergeMode::parse(Some(&m))),
            auto_pr: self.auto_pr,
        }
    }
}

// ---- §6.2 document / branding / asset tools ----------------------------------

/// A `?project=` listing filter shared by `document.list`, `branding.list`, and
/// `asset.list` — the typed twin of the REST [`ProjectQuery`](../../../crates/lazybones-api/src/dto.rs).
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ProjectArgs {
    /// Restrict the listing to one project scope; all scopes when omitted.
    #[serde(default)]
    pub project: Option<String>,
}

/// Arguments for `document.create` — the typed twin of `POST /documents`
/// ([`CreateDocumentBody`](../../../crates/lazybones-api/src/dto.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentCreateArgs {
    /// The unique document id; conflicts if it is taken.
    pub id: String,
    /// Human title.
    pub title: String,
    /// `document` (default) or `reference` (a reusable page merged into others);
    /// any other value parses to `document`.
    #[serde(default)]
    pub kind: Option<String>,
    /// The brand profile to render with; omitted falls back to the default.
    #[serde(default)]
    pub branding_id: Option<String>,
    /// Optional project scope; always `None` today.
    #[serde(default)]
    pub project: Option<String>,
}

/// Arguments for `document.update` — the typed twin of `PUT /documents/:id`
/// ([`UpdateDocumentBody`](../../../crates/lazybones-api/src/dto.rs)). `created_at`
/// and the `repo` linkage are preserved server-side.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentUpdateArgs {
    /// The document id to edit; `404` if unknown.
    pub id: String,
    /// New title.
    pub title: String,
    /// New kind (`document` | `reference`); any other value parses to `document`.
    #[serde(default)]
    pub kind: Option<String>,
    /// New branding id (or omitted to clear).
    #[serde(default)]
    pub branding_id: Option<String>,
}

/// Arguments for `document.add_page` — the typed twin of `POST /documents/:id/pages`
/// ([`CreatePageBody`](../../../crates/lazybones-api/src/dto.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentAddPageArgs {
    /// The document (book) id to add the page to; `404` if unknown.
    pub document_id: String,
    /// Page title / running header.
    #[serde(default)]
    pub title: String,
    /// The page body (markdown).
    #[serde(default)]
    pub body: String,
    /// Explicit fractional sort position; omit to append after the last page.
    #[serde(default)]
    pub position: Option<f64>,
}

/// Arguments for `document.update_page` — the typed twin of
/// `PUT /documents/:id/pages/:pid` ([`UpdatePageBody`](../../../crates/lazybones-api/src/dto.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentUpdatePageArgs {
    /// The document the page belongs to; `404` if the page is not under it.
    pub document_id: String,
    /// The page id to overwrite.
    pub page_id: String,
    /// New title.
    #[serde(default)]
    pub title: String,
    /// New markdown body.
    #[serde(default)]
    pub body: String,
    /// New fractional sort position; omit to leave the page where it is.
    #[serde(default)]
    pub position: Option<f64>,
}

/// Address one document by id — shared by `document.list_pages`,
/// `document.list_references`, and `document.list_sources`.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentRefArgs {
    /// The document id to read; `404` if unknown.
    pub document_id: String,
}

/// Arguments for `document.attach_reference` — the typed twin of
/// `POST /documents/:id/references`
/// ([`AddReferenceBody`](../../../crates/lazybones-api/src/routes/documents.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentAttachReferenceArgs {
    /// The document to merge the reference into; `404` if unknown.
    pub document_id: String,
    /// The id of the `reference` document to merge into this one's output.
    pub reference_id: String,
}

/// Arguments for `document.add_source` — a **link** source, the JSON twin of
/// `POST /documents/:id/sources` ([`LinkSourceBody`](../../../crates/lazybones-api/src/dto.rs)).
/// Asset *bytes* (file uploads) stay on the REST raw-body route; MCP carries only
/// link sources + ids (design §6.2).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentAddSourceArgs {
    /// The document the source sits behind; `404` if unknown.
    pub document_id: String,
    /// The URL the author references.
    pub url: String,
    /// Human title / label; defaults to the URL when blank.
    #[serde(default)]
    pub title: String,
}

/// Arguments for `document.render` — the typed twin of `GET /documents/:id/render`
/// ([`RenderQuery`](../../../crates/lazybones-api/src/routes/document_render.rs)).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentRenderArgs {
    /// The document id to render; `404` if unknown.
    pub id: String,
    /// Preview a brand other than the saved one (blank ⇒ the default brand). Omitted
    /// ⇒ the document's saved `branding_id`.
    #[serde(default)]
    pub branding_id: Option<String>,
    /// Print a page number on every page.
    #[serde(default)]
    pub page_numbers: bool,
    /// Prepend a table-of-contents index page.
    #[serde(default)]
    pub index: bool,
}

/// Arguments for `document.set_repo` — the typed twin of `PUT /documents/:id/repo`
/// ([`SetDocRepoBody`](../../../crates/lazybones-api/src/dto.rs)). The
/// `branch`/`*_url` linkage is filled by the publish flow, not here.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentSetRepoArgs {
    /// The document id whose publishing target to set; `404` if unknown.
    pub id: String,
    /// Absolute path to the local git checkout.
    pub repo: String,
    /// Base branch to fork from; omitted inherits the default.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// Branch-name prefix; omitted inherits the default.
    #[serde(default)]
    pub branch_prefix: Option<String>,
    /// Where in the repo the rendered doc is committed, e.g. `docs/<id>.md`.
    pub output_path: String,
}

/// Arguments for `document.publish` — the typed twin of `POST /documents/:id/publish`
/// ([`PublishBody`](../../../crates/lazybones-api/src/routes/document_gh.rs)): branch
/// → commit → push → PR, in one call.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DocumentPublishArgs {
    /// The document id to publish; `404` if unknown, `400` if it has no repo target.
    pub id: String,
    /// The commit message; omitted defaults to `docs: publish <title>`.
    #[serde(default)]
    pub message: Option<String>,
    /// The PR title; omitted defaults to the document title.
    #[serde(default)]
    pub title: Option<String>,
    /// The PR body; omitted defaults to a generated note.
    #[serde(default)]
    pub body: Option<String>,
}

/// A brand color palette in MCP arg form — the JSON-Schema twin of the store's
/// [`BrandColors`] (which is not `JsonSchema`). All fields are CSS-style color
/// strings; absent fields stay empty (the store/render layer falls back).
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BrandColorsArgs {
    /// The dominant brand color.
    #[serde(default)]
    pub primary: String,
    /// The supporting color.
    #[serde(default)]
    pub secondary: String,
    /// The highlight/call-to-action color.
    #[serde(default)]
    pub accent: String,
    /// Default body-text color.
    #[serde(default)]
    pub text: String,
    /// Page/background color.
    #[serde(default)]
    pub background: String,
}

impl From<BrandColorsArgs> for BrandColors {
    fn from(c: BrandColorsArgs) -> Self {
        BrandColors {
            primary: c.primary,
            secondary: c.secondary,
            accent: c.accent,
            text: c.text,
            background: c.background,
        }
    }
}

/// Brand typography in MCP arg form — the JSON-Schema twin of the store's
/// [`BrandFonts`].
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BrandFontsArgs {
    /// Font family for headings.
    #[serde(default)]
    pub heading: String,
    /// Font family for body text.
    #[serde(default)]
    pub body: String,
}

impl From<BrandFontsArgs> for BrandFonts {
    fn from(f: BrandFontsArgs) -> Self {
        BrandFonts {
            heading: f.heading,
            body: f.body,
        }
    }
}

/// Arguments for `branding.create` / `branding.update` — the typed twin of
/// `POST /branding` / `PUT /branding/:id`
/// ([`CreateBrandingBody`](../../../crates/lazybones-api/src/dto.rs)). On update the
/// `id` names the existing profile (`created_at` preserved server-side).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BrandingArgs {
    /// Unique branding id; create conflicts if taken, update `404`s if unknown.
    pub id: String,
    /// Human name shown in the picker.
    pub name: String,
    /// Optional project scope; always `None` today. Honored on create only.
    #[serde(default)]
    pub project: Option<String>,
    /// The asset id of the brand's logo, if set.
    #[serde(default)]
    pub logo_asset_id: Option<String>,
    /// The brand color palette.
    #[serde(default)]
    pub colors: BrandColorsArgs,
    /// The brand typography.
    #[serde(default)]
    pub fonts: BrandFontsArgs,
    /// Header text rendered on branded output.
    #[serde(default)]
    pub header_text: String,
    /// Footer text rendered on branded output.
    #[serde(default)]
    pub footer_text: String,
}
