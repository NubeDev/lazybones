//! Shared route guard: a task-level revive verb may only act when the parent
//! workflow is in a revivable lifecycle.
//!
//! The task-level revive verbs (`retry`, `auto-retry`, chat-revive) operate
//! directly on a task. Without this guard a `stopped` (paused) workflow's tasks
//! stay revivable — and because resume flips the run back to `active`, a revived
//! task would be re-claimed the moment the run resumes, so "stopped" would lie:
//! the UI says paused while work quietly continues. This refuses those verbs with
//! `409` so the operator must [`resume`](super::workflows_resume) the workflow
//! first. Standalone tasks (no parent run) are always revivable.

use lazybones_store::{Lifecycle, Task};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Refuse (`409`) if `task`'s parent workflow is not in a revivable lifecycle
/// (i.e. it is `stopped`). A task with no parent run, or whose run row is missing,
/// is treated as revivable. Errors only on a store read failure.
pub async fn ensure_run_revivable(state: &AppState, task: &Task) -> ApiResult<()> {
    // Prefer the `run_id` FK, falling back to the `run` label. A standalone task
    // has no matching workflow row, so the `get_run` below returns `None` and it
    // is treated as revivable.
    let run_key = task.run_id.as_deref().unwrap_or(task.run.as_str());

    let Some(run) = state.store.get_run(run_key).await? else {
        // No workflow row — a standalone task; nothing to gate on.
        return Ok(());
    };

    if run.lifecycle != Lifecycle::Active {
        return Err(ApiError::conflict(format!(
            "workflow `{run_key}` is {}; resume it before reviving task `{}`",
            run.lifecycle.as_str(),
            task.id
        )));
    }
    Ok(())
}
