//! HTTP route table: one file per route (SCOPE.md REST surface).
//!
//! Memory (`POST /memory`, `GET /memory/recall`) is intentionally not wired yet —
//! the embedding provider is an open question (SCOPE.md OQ7); the store declares
//! the `memory` table so it can land behind this same router without a migration.

mod activity;
mod agent_catalog;
mod agent_chat;
mod agent_test;
mod agents;
mod assets;
mod block;
mod branding;
mod cancel;
mod chat;
mod claim;
mod content_sync;
mod create;
mod delete;
mod document_gh;
mod document_pages;
mod document_render;
mod document_sources;
mod documents;
mod done;
mod engine;
mod extensions;
mod files;
mod follow_ups;
mod fs_list;
mod gate;
mod get;
mod gh;
mod guard;
mod hcom_log;
mod health;
mod heartbeat;
mod issue;
mod jobs;
mod list;
mod management_agent;
mod preferences;
mod promote;
mod ready;
mod runs;
mod secrets_delete;
mod secrets_env;
mod secrets_list;
mod secrets_put;
mod skills_create;
mod skills_delete;
mod skills_get;
mod skills_list;
mod skills_update;
mod stream;
mod sync;
mod tasks_retry;
mod tasks_retry_policy;
mod template_attachments;
mod templates_create;
mod templates_delete;
mod templates_get;
mod templates_list;
mod templates_update;
mod transcript;
mod update;
mod workflows_add_task;
mod workflows_create;
mod workflows_delete;
mod workflows_get;
mod workflows_list;
mod workflows_restart;
mod workflows_resume;
mod workflows_start;
mod workflows_stop;
mod workflows_stop_reset;
mod workflows_tasks;
mod workflows_update;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post, put};

use crate::cors::cors_layer;
use crate::state::AppState;

/// Assemble the full route table over the shared [`AppState`].
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/workfile/sync", post(sync::sync_workfile))
        .route("/tasks", get(list::list_tasks).post(create::create_task))
        .route("/tasks/promote", post(promote::promote_ready))
        .route("/tasks/:id/ready", post(ready::ready_task))
        .route(
            "/tasks/:id",
            get(get::get_task)
                .patch(update::update_task)
                .delete(delete::delete_task),
        )
        .route("/tasks/:id/claim", post(claim::claim_task))
        .route("/tasks/:id/heartbeat", post(heartbeat::heartbeat))
        .route("/tasks/:id/activity", post(activity::report_activity))
        .route("/tasks/:id/gate", post(gate::gate_task))
        .route("/tasks/:id/done", post(done::done_task))
        .route("/tasks/:id/block", post(block::block_task))
        // Operator cancel: kill the live agent (hcom) then block the task.
        .route("/tasks/:id/cancel", post(cancel::cancel_task))
        // Revive ONE blocked task: guided (strategy → revive in place) or clean
        // (no strategy → reset to pending). The next tick picks it back up.
        .route("/tasks/:id/retry", post(tasks_retry::retry_task))
        // Set/clear a task's hands-off auto-retry policy (strategy + cap).
        .route(
            "/tasks/:id/auto-retry",
            put(tasks_retry_policy::set_auto_retry),
        )
        // Chat with the task's agent: read the conversation + post a message
        // (live-steer a running task, or revive a blocked one to workshop it).
        .route("/tasks/:id/chat", get(chat::get_chat).post(chat::post_chat))
        // GitHub issue linkage: create from the task, link an existing one, or
        // unlink; plus the close-on-done toggle. Backend-first (no UI yet).
        .route(
            "/tasks/:id/issue",
            post(issue::create_issue).delete(issue::unlink_issue),
        )
        .route("/tasks/:id/issue/link", post(issue::link_issue))
        .route(
            "/tasks/:id/issue/close-on-done",
            put(issue::set_close_on_done),
        )
        // The fabric's record: one agent's raw hcom log + its deep transcript.
        .route("/tasks/:id/hcom", get(hcom_log::task_hcom_log))
        .route("/tasks/:id/transcript", get(transcript::task_transcript))
        // Reusable task templates (global, stateless recipes).
        .route(
            "/templates",
            get(templates_list::list_templates).post(templates_create::create_template),
        )
        .route(
            "/templates/:id",
            get(templates_get::get_template)
                .put(templates_update::update_template)
                .delete(templates_delete::delete_template),
        )
        // Generic attachments on a template (first owner of the polymorphic seam):
        // attach/list skills (or any thing-kind) to a template.
        .route(
            "/templates/:id/attachments",
            get(template_attachments::list_template_attachments)
                .post(template_attachments::attach_to_template),
        )
        .route(
            "/templates/:id/attachments/:thing_kind/:thing_id",
            delete(template_attachments::detach_from_template),
        )
        // Documents: authored, branded markdown documents (+ reusable reference
        // pages). Reads open; mutations + GitHub publishing guarded by `Document`.
        .route(
            "/documents",
            get(documents::list_documents).post(documents::create_document),
        )
        .route(
            "/documents/:id",
            get(documents::get_document)
                .put(documents::update_document)
                .delete(documents::delete_document),
        )
        // Reusable reference pages merged into a document's rendered output, over
        // the generic attachment seam (`thing_kind="reference"`).
        .route(
            "/documents/:id/references",
            get(documents::list_references).post(documents::add_reference),
        )
        .route(
            "/documents/:id/references/:ref_id",
            delete(documents::remove_reference),
        )
        // Render: assembled HTML preview + PDF export.
        // Pages: the ordered content of a document/book. Reads open; mutations
        // (incl. reorder via the `position` field) guarded by `Document`.
        .route(
            "/documents/:id/pages",
            get(document_pages::list_pages).post(document_pages::create_page),
        )
        .route(
            "/documents/:id/pages/:pid",
            get(document_pages::get_page)
                .put(document_pages::update_page)
                .delete(document_pages::delete_page),
        )
        .route("/documents/:id/render", get(document_render::render_document))
        .route("/documents/:id/export.pdf", get(document_render::export_pdf))
        // Sources: a document's uploads / context material (links + files), behind
        // the doc and never rendered. Files ride the blob store + sha256 dedup.
        .route(
            "/documents/:id/sources",
            get(document_sources::list_sources).post(document_sources::add_source),
        )
        .route(
            "/documents/:id/sources/:sid",
            delete(document_sources::remove_source),
        )
        // GitHub publishing: set a repo target, then branch → commit → PR/issue
        // (or one-call publish). All guarded by `Document`.
        .route("/documents/:id/repo", put(document_gh::set_repo))
        .route("/documents/:id/gh/branch", post(document_gh::create_branch))
        .route("/documents/:id/gh/commit", post(document_gh::commit))
        .route("/documents/:id/gh/pr", post(document_gh::create_pr))
        .route("/documents/:id/gh/issue", post(document_gh::create_issue))
        .route("/documents/:id/publish", post(document_gh::publish))
        // Assets: the content-addressed file server (logos, images). Reads open
        // (the logo/image source); mutations guarded by `Document`.
        .route(
            "/assets",
            get(assets::list_assets).post(assets::create_asset),
        )
        .route(
            "/assets/:id",
            get(assets::get_asset).delete(assets::delete_asset),
        )
        // Branding: the standalone, app-wide brand-profile catalogue.
        .route(
            "/branding",
            get(branding::list_branding).post(branding::create_branding),
        )
        .route(
            "/branding/:id",
            get(branding::get_branding)
                .put(branding::update_branding)
                .delete(branding::delete_branding),
        )
        // Skills: reusable blocks of agent instructions (global, stateless).
        .route(
            "/skills",
            get(skills_list::list_skills).post(skills_create::create_skill),
        )
        .route(
            "/skills/:id",
            get(skills_get::get_skill)
                .put(skills_update::update_skill)
                .delete(skills_delete::delete_skill),
        )
        // The Lazybones Agent: a conversational operator aide. Its config lives in
        // settings; it authors/reads through the same REST surface a human uses and
        // never starts or runs anything (docs/agent/lazybones-agent-scope.md).
        .route(
            "/settings/management-agent",
            get(management_agent::get_management_agent).put(management_agent::put_management_agent),
        )
        // Per-workflow config overrides (resolution is override ?? global).
        .route(
            "/settings/management-agent/workflows/:id",
            get(management_agent::get_workflow_management_agent)
                .put(management_agent::put_workflow_management_agent)
                .delete(management_agent::delete_workflow_management_agent),
        )
        // User preferences: operator UI choices (timezone, theme) that follow
        // the operator across browsers, rather than living in localStorage.
        .route(
            "/settings/preferences",
            get(preferences::get_preferences).put(preferences::put_preferences),
        )
        // Content sync: git-backed sync of authored docs/skills/tasks/templates/
        // workflows between machines. Status drives the "out of sync — pull?"
        // banner; pull/push run through the generic job runner.
        .route("/content-sync/status", get(content_sync::get_status))
        .route("/content-sync/pull", post(content_sync::post_pull))
        .route("/content-sync/push", post(content_sync::post_push))
        // The generic job-runner surface (content-sync jobs live here).
        .route("/jobs", get(jobs::list_jobs))
        .route("/jobs/:name", post(jobs::run_job))
        .route("/agent/chat", post(agent_chat::post_agent_chat))
        .route("/agent/chat/:conversation", get(agent_chat::get_agent_chat))
        .route(
            "/agent/chat/:conversation/stream",
            get(agent_chat::agent_chat_stream),
        )
        // Stop the agent running this conversation's turn (kill its hcom agent).
        .route(
            "/agent/chat/:conversation/stop",
            post(agent_chat::stop_agent_chat),
        )
        .route(
            "/agent/conversations",
            get(agent_chat::list_agent_conversations),
        )
        // Workflows (one-off runs, stored in the `run` table; path stays user-facing).
        .route(
            "/workflows",
            get(workflows_list::list_workflows).post(workflows_create::create_workflow),
        )
        .route(
            "/workflows/:id",
            get(workflows_get::get_workflow)
                .patch(workflows_update::update_workflow)
                .delete(workflows_delete::delete_workflow),
        )
        .route(
            "/workflows/:id/tasks",
            get(workflows_tasks::list_workflow_tasks).post(workflows_add_task::add_workflow_task),
        )
        .route(
            "/workflows/:id/start",
            post(workflows_start::start_workflow),
        )
        // Stop (pause): lifecycle → stopped; reclaim running tasks to ready, keep
        // all work. The scheduler then promotes/claims nothing for this run.
        .route("/workflows/:id/stop", post(workflows_stop::stop_workflow))
        // Stop & reset: pause AND reset unfinished tasks to pending (throw in-flight
        // progress away). Still resumable — not a terminal tombstone.
        .route(
            "/workflows/:id/stop-reset",
            post(workflows_stop_reset::stop_reset_workflow),
        )
        .route(
            "/workflows/:id/restart",
            post(workflows_restart::restart_workflow),
        )
        // Resume (un-pause): lifecycle → active + reset blocked tasks → pending, so
        // the scheduler continues from where it left off.
        .route(
            "/workflows/:id/resume",
            post(workflows_resume::resume_workflow),
        )
        .route("/runs/:id", get(runs::run_history))
        // The fabric's record for a whole run: the raw hcom log of every agent.
        .route("/runs/:id/hcom", get(hcom_log::run_hcom_log))
        // Follow-ups: the "needs a human" surface. The scheduler files these when
        // it hits a wall it can't clear; agents file their own; operators resolve.
        .route("/runs/:id/follow-ups", get(follow_ups::run_follow_ups))
        .route("/follow-ups", post(follow_ups::file_follow_up))
        .route(
            "/follow-ups/:id/resolve",
            post(follow_ups::resolve_follow_up),
        )
        // Live push feed of status transitions (SSE) — for the dashboard + loop.
        .route("/stream", get(stream::stream))
        // Native filesystem browse for the UI's repo/dir picker (New workflow).
        .route("/fs/list", get(fs_list::fs_list))
        // Read-only repo file browser + diff for a workflow's "Files" tab.
        .route("/files/tree", get(files::list_tree))
        .route("/files/read", get(files::read_file))
        .route("/files/diff", get(files::diff))
        // GitHub via the user's existing `gh`/`git` login (no token here).
        .route("/gh/auth", get(gh::gh_auth))
        .route("/gh/repo", get(gh::gh_repo))
        .route(
            "/gh/branches",
            get(gh::gh_branches).post(gh::gh_create_branch),
        )
        .route("/gh/local-branches", get(gh::gh_branches_local))
        .route("/gh/branches/:name", delete(gh::gh_delete_branch))
        .route("/gh/checkout", post(gh::gh_checkout))
        .route(
            "/gh/worktrees",
            get(gh::gh_worktrees).delete(gh::gh_remove_worktree),
        )
        .route("/gh/worktrees/prune", post(gh::gh_prune_worktrees))
        .route("/gh/issues", get(gh::gh_issues).post(gh::gh_create_issue))
        .route("/gh/mentionable", get(gh::gh_mentionable))
        .route("/gh/issues/:number", get(gh::gh_issue_view))
        .route("/gh/issues/:number/close", post(gh::gh_close_issue))
        .route(
            "/gh/issues/:number/comments",
            get(gh::gh_issue_comments).post(gh::gh_comment_issue),
        )
        .route("/gh/prs", get(gh::gh_prs).post(gh::gh_create_pr))
        .route("/gh/prs/:number", get(gh::gh_pr_view))
        .route("/gh/prs/:number/merge", post(gh::gh_merge_pr))
        .route("/gh/prs/:number/close", post(gh::gh_close_pr))
        .route(
            "/gh/prs/:number/comments",
            get(gh::gh_pr_comments).post(gh::gh_comment_pr),
        )
        // Engine + agent availability (so the UI can show what's set up).
        .route("/engine", get(engine::engine_status))
        .route("/agents", get(agents::list_agents))
        // Live-test one agent's credential by launching it through hcom.
        .route("/agents/:tool/test", post(agent_test::test_agent_route))
        // The CRUD-able agent catalog: agent CLIs + their model/effort menus,
        // seeded with defaults and editable by an operator. Drives the add-task
        // UI's agent/model/effort pickers. Reads open; mutations loop-only.
        .route(
            "/agent-catalog",
            get(agent_catalog::list_agents).post(agent_catalog::create_agent),
        )
        .route(
            "/agent-catalog/:id",
            get(agent_catalog::get_agent)
                .patch(agent_catalog::update_agent)
                .delete(agent_catalog::delete_agent),
        )
        // The secret store: agent CLI credentials, encrypted at rest. The `env`
        // export (decrypts) and all mutations are loop-guarded by `Secret`.
        .route("/secrets", get(secrets_list::list_secrets))
        .route("/secrets/env", get(secrets_env::secret_env))
        .route(
            "/secrets/:tool",
            put(secrets_put::put_secret).delete(secrets_delete::delete_secret),
        )
        // Backend WASM extensions (design §3.6): install (upload/url) + CRUD,
        // enable/disable, capability grants, and a manual/test invoke. Reads are
        // open; mutations require the loop-only `Extension` capability.
        .route(
            "/extensions",
            get(extensions::list_extensions).post(extensions::install_extension),
        )
        .route(
            "/extensions/:id",
            get(extensions::get_extension).delete(extensions::delete_extension),
        )
        .route("/extensions/:id/enable", post(extensions::enable_extension))
        .route(
            "/extensions/:id/disable",
            post(extensions::disable_extension),
        )
        .route("/extensions/:id/grants", post(extensions::set_grants))
        .route("/extensions/:id/invoke", post(extensions::invoke_extension))
        // Frontend asset proxy (design §4.3): serve an enabled extension's
        // federated remote bundle (remoteEntry.js + chunks) from blobs. Open read.
        .route(
            "/extensions/:id/frontend/*path",
            get(extensions::frontend_asset),
        )
        // Raw-body uploads (asset logos/images, PDF sources) routinely exceed
        // axum's 2 MB default body limit; lift it so real images/PDFs upload.
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        // Let the browser/desktop UI (a different origin) read the surface.
        .layer(cors_layer())
        .with_state(state)
}
