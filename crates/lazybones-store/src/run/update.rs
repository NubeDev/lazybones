//! Update a workflow's workspace defaults (the inheritable git + agent config).
//!
//! Overwrites the workspace block of an existing run, preserving everything else
//! (lifecycle, timestamps, hcom cursor). The `repo` is intentionally NOT editable
//! here — re-pointing a live workflow at a different repo would orphan its tasks'
//! worktrees — so callers pass the full new [`Workspace`] but the existing repo is
//! kept. Tasks pick up the new defaults on their next claim (most-specific-wins in
//! the engine's `EffectiveGit` resolver), so an edit never disturbs running work.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::get::get_run;
use super::model::{Run, Workspace};
use super::row::{RUN_TABLE, RunRow};

/// Overwrite `run:<id>`'s workspace defaults, keeping its `repo`, lifecycle and
/// timestamps. Returns the updated run.
///
/// # Errors
/// Returns [`StoreError::RunNotFound`] if the run does not exist, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_workspace(db: &Surreal<Db>, id: &str, workspace: Workspace) -> Result<Run> {
    let mut run = get_run(db, id)
        .await?
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))?;

    // Keep the original repo — only the inheritable defaults are editable.
    let repo = run.workspace.repo.clone();
    run.workspace = Workspace { repo, ..workspace };

    let written: Option<RunRow> = db
        .update((RUN_TABLE, id.to_owned()))
        .content(RunRow::from_run(&run))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(RunRow::into_run)
        .ok_or_else(|| StoreError::RunNotFound(id.to_owned()))
}
