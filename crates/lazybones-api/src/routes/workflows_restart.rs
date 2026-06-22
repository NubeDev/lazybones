//! `POST /workflows/:id/restart` — reset a workflow to run from the beginning.
//!
//! A restart is a deliberate operator override: it forces the workflow's tasks
//! back to `pending` (clearing each one's claim/worktree/commit/heartbeat), so
//! the run derives back to `draft` and can be `start`ed again. It does **not**
//! re-promote roots — the operator presses Start when ready (two explicit steps).
//!
//! By default a restart is a **true hard reset**: it re-runs *everything* from
//! scratch (done tasks included), tears down every task's worktree, and deletes the
//! workflow's task branch(es) both locally and on the remote — so the next run
//! starts from a clean base with no leftover commits or branches. `master`/base is
//! never force-reset; deleting the workflow's own branch is enough.
//!
//! One flag softens it:
//! - `soft`: a resume-style restart. Keeps `done` tasks (only the unfinished part
//!   — running/gating/blocked/ready/pending — is reset) and keeps each task's
//!   worktree + branch for the scheduler to reuse. Use it to retry the tail of a
//!   run without redoing committed work.
//!
//! Live agents (`running`/`gating`) are always killed first (best-effort) so a
//! reset task doesn't leave an orphaned agent. Requires `Block` (it discards
//! in-flight work, same authority as cancel).

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{Status, StoreError};
use serde::Deserialize;

use crate::dto::WorkflowSummary;
use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// Restart options. The default (empty body) is a **hard reset**: re-run
/// everything, remove worktrees, delete the workflow's branch(es) local + remote.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RestartBody {
    /// Soften to a resume-style restart: keep `done` tasks (reset only the
    /// unfinished part) and keep each task's worktree + branch. Default `false`
    /// (a full hard reset).
    pub soft: bool,
}

/// Restart workflow `:id`: kill live agents, optionally remove worktrees, reset
/// its tasks to `pending`. `404` if the workflow is unknown.
pub async fn restart_workflow(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    body: Option<Json<RestartBody>>,
) -> ApiResult<Json<WorkflowSummary>> {
    session.require(Capability::Block, "block", &id)?;
    let Json(opts) = body.unwrap_or_default();
    // Default is a full hard reset; `soft` is the resume-style escape hatch.
    let hard = !opts.soft;

    let run = state
        .store
        .get_run(&id)
        .await?
        .ok_or(StoreError::RunNotFound(id.clone()))?;
    let repo = std::path::Path::new(&run.workspace.repo);
    // The remote each task branch was pushed to — same source `EngineConfig` reads.
    let remote = std::env::var("LAZYBONES_REMOTE").unwrap_or_else(|_| "origin".to_owned());

    // On a hard reset, tear down each task's tree and delete its branch (local +
    // remote). `Shared` mode puts many tasks on ONE branch (`<prefix><run_id>`), so
    // dedupe by branch to delete it once, not once per task.
    let mut wiped_branches: std::collections::HashSet<String> = std::collections::HashSet::new();

    let tasks = state.store.list_run_tasks(&id).await?;
    for task in &tasks {
        // Keep done tasks on a soft restart; a hard reset redoes them from scratch.
        if task.status == Status::Done && !hard {
            continue;
        }
        // Already at the start with nothing to clear — skip the churn/event.
        if task.status == Status::Pending {
            continue;
        }

        // Kill the live agent first so a claimed task doesn't leave an orphan.
        if matches!(task.status, Status::Running | Status::Gating)
            && let Err(e) = lazybones_engine::cancel_agent(&task.id).await
        {
            tracing::warn!(task = %task.id, "restart: hcom kill failed (continuing): {e}");
        }

        // Hard reset: tear down the worktree and delete its branch (local + remote)
        // before we forget their values on reset. The branch must go too, or the
        // next New-mode claim's `worktree add -b <branch>` collides with the
        // leftover; the remote delete clears a pushed PR branch so the re-run is
        // clean. Dedupe per branch (Shared shares one across tasks).
        if hard && let Some(path) = &task.worktree {
            let branch = task.branch.as_deref();
            let fresh_branch = branch.is_some_and(|b| wiped_branches.insert(b.to_owned()));
            // Only ask remove_worktree to delete the branch once per branch name;
            // for the 2nd+ Shared task the branch is already gone.
            let branch_arg = if fresh_branch { branch } else { None };
            if let Err(e) =
                lazybones_engine::remove_worktree(repo, path, branch_arg, Some(&remote)).await
            {
                tracing::warn!(task = %task.id, "restart: worktree remove failed (continuing): {e}");
            }
        }

        if let Err(e) = state.store.reset(&task.id, session.actor()).await {
            tracing::warn!(task = %task.id, "restart: reset failed: {e}");
        }
    }

    // Deterministic final sweep for `Shared` mode. A shared run has ONE
    // worktree+branch keyed by the *run id* (`<prefix><run_id>` at
    // `<repo>/<root>/<run_id>`), but after a partial/failed run that branch may
    // exist with NO task still carrying `task.worktree`/`task.branch` (e.g. only
    // the first task was ever claimed). The per-task loop above would then never
    // clean it, leaving the polluted branch + tree behind — exactly the bug a hard
    // reset must not have. So on a hard reset we also tear down the run's shared
    // branch/tree by its derived name, regardless of task state. Idempotent: if the
    // per-task loop already removed it, this is a no-op (absent tree/branch is fine).
    if hard && run.workspace.worktree_mode == lazybones_store::WorktreeMode::Shared {
        let prefix = run
            .workspace
            .branch_prefix
            .clone()
            .or_else(|| std::env::var("LAZYBONES_BRANCH_PREFIX").ok())
            .unwrap_or_else(|| "lazy/".to_owned());
        let root = std::env::var("LAZYBONES_WORKTREE_ROOT").unwrap_or_else(|_| ".lazy/wt".to_owned());
        let branch = format!("{prefix}{id}");
        if !wiped_branches.contains(&branch) {
            let path = repo.join(&root).join(&id);
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

    // Un-activate the run: clear `started_at` (and force lifecycle `Active`). The
    // scheduler promotes roots for any `Active` run that has a `started_at`, so
    // without this a previously-started workflow re-runs on the very next tick —
    // defeating the "does not auto-start; press Start when ready" contract. The
    // next `start` re-stamps it. Best-effort: a failure here only means the run
    // stays activated, which a subsequent stop/start still recovers.
    let run = match state.store.clear_run_started(&id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(run = %id, "restart: clear started_at failed (continuing): {e}");
            run
        }
    };

    // Re-read so the summary reflects the post-restart task statuses.
    let tasks = state.store.list_run_tasks(&id).await?;
    Ok(Json(WorkflowSummary::new(run, &tasks)))
}
