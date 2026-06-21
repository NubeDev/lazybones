//! HTTP route table: one file per route (SCOPE.md REST surface).
//!
//! Memory (`POST /memory`, `GET /memory/recall`) is intentionally not wired yet —
//! the embedding provider is an open question (SCOPE.md OQ7); the store declares
//! the `memory` table so it can land behind this same router without a migration.

mod activity;
mod agent_catalog;
mod agent_test;
mod agents;
mod block;
mod cancel;
mod chat;
mod claim;
mod create;
mod delete;
mod done;
mod engine;
mod files;
mod fs_list;
mod gate;
mod follow_ups;
mod get;
mod gh;
mod guard;
mod hcom_log;
mod health;
mod heartbeat;
mod list;
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

use axum::Router;
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
        .route("/tasks/:id/auto-retry", put(tasks_retry_policy::set_auto_retry))
        // Chat with the task's agent: read the conversation + post a message
        // (live-steer a running task, or revive a blocked one to workshop it).
        .route(
            "/tasks/:id/chat",
            get(chat::get_chat).post(chat::post_chat),
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
        // Workflows (one-off runs, stored in the `run` table; path stays user-facing).
        .route(
            "/workflows",
            get(workflows_list::list_workflows).post(workflows_create::create_workflow),
        )
        .route(
            "/workflows/:id",
            get(workflows_get::get_workflow).delete(workflows_delete::delete_workflow),
        )
        .route(
            "/workflows/:id/tasks",
            get(workflows_tasks::list_workflow_tasks)
                .post(workflows_add_task::add_workflow_task),
        )
        .route("/workflows/:id/start", post(workflows_start::start_workflow))
        // Stop (pause): lifecycle → stopped; reclaim running tasks to ready, keep
        // all work. The scheduler then promotes/claims nothing for this run.
        .route("/workflows/:id/stop", post(workflows_stop::stop_workflow))
        // Stop & reset: pause AND reset unfinished tasks to pending (throw in-flight
        // progress away). Still resumable — not a terminal tombstone.
        .route(
            "/workflows/:id/stop-reset",
            post(workflows_stop_reset::stop_reset_workflow),
        )
        .route("/workflows/:id/restart", post(workflows_restart::restart_workflow))
        // Resume (un-pause): lifecycle → active + reset blocked tasks → pending, so
        // the scheduler continues from where it left off.
        .route("/workflows/:id/resume", post(workflows_resume::resume_workflow))
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
        .route(
            "/gh/issues",
            get(gh::gh_issues).post(gh::gh_create_issue),
        )
        .route("/gh/mentionable", get(gh::gh_mentionable))
        .route("/gh/issues/:number", get(gh::gh_issue_view))
        .route("/gh/issues/:number/close", post(gh::gh_close_issue))
        .route(
            "/gh/issues/:number/comments",
            get(gh::gh_issue_comments).post(gh::gh_comment_issue),
        )
        .route("/gh/prs", get(gh::gh_prs))
        .route("/gh/prs/:number", get(gh::gh_pr_view))
        .route("/gh/prs/:number/merge", post(gh::gh_merge_pr))
        .route("/gh/prs/:number/close", post(gh::gh_close_pr))
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
        // Let the browser/desktop UI (a different origin) read the surface.
        .layer(cors_layer())
        .with_state(state)
}
