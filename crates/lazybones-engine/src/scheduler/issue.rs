//! GitHub issue linkage for tasks: the three operator actions (create / link /
//! unlink), the close-on-done hook, and the reverse issue→task poll.
//!
//! This lives in the engine, not the store, for one reason: the store layer must
//! never shell out, and every action here drives the `gh` CLI via
//! [`lazybones_gh::Gh`]. The store owns the *fields* ([`issue_url`],
//! [`issue_close_on_done`], [`issue_synced_state`]); this module owns the
//! *behaviour* that reads/writes them against live GitHub state.
//!
//! **Repo targeting.** Every `gh` issue method takes a `dir`; running `gh` inside
//! the repo checkout lets it infer `owner/repo` from the remote — no slug
//! parsing. A task's repo is resolved by `task.run_id → Run.workspace.repo`. A
//! **standalone task (`run_id == None`) has no repo**, so issue actions are
//! rejected with [`IssueError::Standalone`].
//!
//! [`issue_url`]: lazybones_store::Task::issue_url
//! [`issue_close_on_done`]: lazybones_store::Task::issue_close_on_done
//! [`issue_synced_state`]: lazybones_store::Task::issue_synced_state

use lazybones_gh::Gh;
use lazybones_store::{IssueSyncState, StoreHandle, Task, Transition, issue_number_from_url};

/// The actor recorded on transitions the reverse-sync drives.
const ACTOR: &str = "scheduler:issue-sync";

/// Why an issue action could not be performed. Surfaced verbatim by the API
/// handlers so an operator sees a clear cause (no `gh`, no repo, bad link).
#[derive(Debug, thiserror::Error)]
pub enum IssueError {
    /// The task has no parent run, so no repo to target. Issue actions need a
    /// workflow (the repo always comes from the workspace, never the task).
    #[error("task `{0}` is standalone (no workflow); issue actions need a repo")]
    Standalone(String),
    /// No such task.
    #[error("task `{0}` not found")]
    TaskNotFound(String),
    /// The task is not linked to an issue (for actions that require one).
    #[error("task `{0}` has no linked issue")]
    NotLinked(String),
    /// The supplied link could not be parsed into an issue number.
    #[error("`{0}` is not a valid issue URL or #number")]
    BadLink(String),
    /// `gh` is missing or unauthenticated (`gh auth login` not run).
    #[error("GitHub CLI unavailable or not authenticated: {0}")]
    Auth(String),
    /// A `gh`/`git` call failed.
    #[error("github error: {0}")]
    Gh(#[from] lazybones_gh::GhError),
    /// A store read/write failed.
    #[error("store error: {0}")]
    Store(#[from] lazybones_store::StoreError),
}

/// Resolve the repo dir a task's issue actions target: `run_id →
/// Run.workspace.repo`. A standalone task (no run) is an error — its issue
/// actions are rejected, never silently pointed at the global repo.
async fn repo_dir(store: &StoreHandle, task: &Task) -> Result<String, IssueError> {
    let Some(run_id) = task.run_id.as_deref() else {
        return Err(IssueError::Standalone(task.id.clone()));
    };
    match store.get_run(run_id).await? {
        Some(run) => Ok(run.workspace.repo),
        None => Err(IssueError::Standalone(task.id.clone())),
    }
}

/// Load a task or fail with [`IssueError::TaskNotFound`].
async fn load(store: &StoreHandle, id: &str) -> Result<Task, IssueError> {
    store
        .get_task(id)
        .await?
        .ok_or_else(|| IssueError::TaskNotFound(id.to_owned()))
}

/// Pre-flight: a missing / unauthed `gh` becomes a clear, surfaced error rather
/// than a confusing per-command failure later.
async fn ensure_auth(gh: &Gh) -> Result<(), IssueError> {
    gh.ensure_auth()
        .await
        .map_err(|e| IssueError::Auth(e.to_string()))
}

/// Persist the linkage fields on `task` without disturbing its lifecycle or
/// authored fields. Uses a targeted re-`upsert` of the loaded-then-mutated task;
/// `upsert` preserves runtime state, and we are mutating exactly the runtime
/// issue fields here.
async fn save_link(store: &StoreHandle, task: &Task) -> Result<Task, IssueError> {
    Ok(store.set_issue_link(task).await?)
}

/// **Create** a fresh GitHub issue from the task's title + spec, link it, and
/// record it as `Open`.
///
/// # Errors
/// [`IssueError::Standalone`] for a run-less task; [`IssueError::Auth`] if `gh`
/// is unavailable; [`IssueError::Gh`] / [`IssueError::Store`] on a call failure.
pub async fn create(store: &StoreHandle, gh: &Gh, id: &str) -> Result<Task, IssueError> {
    let mut task = load(store, id).await?;
    let dir = repo_dir(store, &task).await?;
    ensure_auth(gh).await?;

    let url = gh.issue_create(&dir, &task.title, &task.spec).await?;
    task.issue_url = Some(url);
    task.issue_synced_state = Some(IssueSyncState::Open);
    save_link(store, &task).await
}

/// **Link** an existing issue by URL or `#number`. Validates it resolves via
/// `issue_view`, then stores the canonical URL + its current state.
///
/// # Errors
/// [`IssueError::BadLink`] if `link` has no parseable number;
/// [`IssueError::Standalone`] / [`IssueError::Auth`] / [`IssueError::Gh`] as above.
pub async fn link(store: &StoreHandle, gh: &Gh, id: &str, link: &str) -> Result<Task, IssueError> {
    let mut task = load(store, id).await?;
    let dir = repo_dir(store, &task).await?;
    ensure_auth(gh).await?;

    let number = parse_link(link).ok_or_else(|| IssueError::BadLink(link.to_owned()))?;
    // Validate the issue resolves, and capture its canonical URL + live state.
    let issue = gh.issue_view(&dir, number).await?;
    task.issue_url = Some(if issue.url.is_empty() {
        link.to_owned()
    } else {
        issue.url.clone()
    });
    task.issue_synced_state = IssueSyncState::parse(Some(&issue.state));
    save_link(store, &task).await
}

/// **Unlink**: clear all three issue fields. Does **not** touch the GitHub issue.
///
/// # Errors
/// [`IssueError::TaskNotFound`] / [`IssueError::Store`] on a store failure.
pub async fn unlink(store: &StoreHandle, id: &str) -> Result<Task, IssueError> {
    let mut task = load(store, id).await?;
    task.issue_url = None;
    task.issue_close_on_done = false;
    task.issue_synced_state = None;
    save_link(store, &task).await
}

/// Close-on-done hook (task → issue), called from `finish` after a task commits
/// to `done`. Best-effort: a failure logs a warning and never blocks or reverts
/// the task. No-op unless the task is both linked and opted into close-on-done.
///
/// Sets `issue_synced_state = Closed` after a successful close so the reverse
/// poll recognises this as a close *we* initiated and never bounces it back.
pub async fn close_on_done(store: &StoreHandle, gh: &Gh, task: &Task) {
    if !task.issue_close_on_done {
        return;
    }
    let Some(url) = task.issue_url.as_deref() else {
        return;
    };
    let Some(number) = issue_number_from_url(url) else {
        tracing::warn!(task = %task.id, %url, "close-on-done: unparseable issue url, skipping");
        return;
    };
    let dir = match repo_dir(store, task).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(task = %task.id, "close-on-done: repo unresolved, skipping: {e}");
            return;
        }
    };
    if let Err(e) = gh.issue_close(&dir, number).await {
        tracing::warn!(task = %task.id, "close-on-done: issue close failed (non-blocking): {e}");
        return;
    }
    // Record the close so the reverse poll treats it as already-synced.
    let mut updated = task.clone();
    updated.issue_synced_state = Some(IssueSyncState::Closed);
    if let Err(e) = store.set_issue_link(&updated).await {
        tracing::warn!(task = %task.id, "close-on-done: recording synced state failed: {e}");
    }
    tracing::info!(task = %task.id, number, "close-on-done: closed linked issue");
}

/// Reverse sync (issue → task): for every linked task, view the live issue and
/// reconcile a *state change* into the task lifecycle.
///
/// - **Issue closed on GitHub** (synced != Closed && live closed) **and the task
///   is not already `done`** → land the task `done` (commit-less external
///   completion). The issue is the source of truth for "no longer needed".
/// - **Issue reopened** (synced == Closed && live open) → revive the task if it
///   is in a terminal/blocked state (mirrors the manual revive); otherwise just
///   record the state.
/// - No diff → nothing to do.
///
/// Best-effort throughout: every task is independent and a single failure is
/// logged and skipped, so the poll never wedges the tick (it runs after
/// `hcom_tail`, never blocking claim/spawn).
pub async fn reverse_sync(store: &StoreHandle, gh: &Gh) {
    let linked = match linked_tasks(store).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("issue reverse-sync: listing tasks failed: {e}");
            return;
        }
    };
    if linked.is_empty() {
        return;
    }
    // One auth pre-flight for the whole pass; if `gh` is down, skip quietly
    // (this is a background best-effort step, not an operator action).
    if gh.ensure_auth().await.is_err() {
        tracing::debug!("issue reverse-sync: gh unauthenticated, skipping pass");
        return;
    }

    for task in linked {
        if let Err(e) = sync_one(store, gh, &task).await {
            tracing::warn!(task = %task.id, "issue reverse-sync: {e}");
        }
    }
}

/// Reconcile a single linked task against its live issue state.
async fn sync_one(store: &StoreHandle, gh: &Gh, task: &Task) -> Result<(), IssueError> {
    let Some(url) = task.issue_url.as_deref() else {
        return Ok(());
    };
    let Some(number) = issue_number_from_url(url) else {
        return Ok(()); // Unparseable url: nothing to poll.
    };
    let dir = repo_dir(store, task).await?;
    let issue = gh.issue_view(&dir, number).await?;
    let live = IssueSyncState::parse(Some(&issue.state));
    let synced = task.issue_synced_state;

    match (synced, live) {
        // Issue closed externally and we haven't already recorded it → land done.
        (s, Some(IssueSyncState::Closed)) if s != Some(IssueSyncState::Closed) => {
            if task.status != lazybones_store::Status::Done {
                match store
                    .transition(&task.id, Transition::ExternalDone, ACTOR)
                    .await
                {
                    Ok(_) => tracing::info!(
                        task = %task.id, number,
                        "issue closed on GitHub → task landed done"
                    ),
                    // The task may have completed/blocked concurrently; record the
                    // state anyway so we don't re-attempt every pass.
                    Err(e) => tracing::info!(
                        task = %task.id,
                        "issue-close transition skipped ({e}); recording state only"
                    ),
                }
            }
            record_state(store, task, IssueSyncState::Closed).await
        }
        // Issue reopened after we'd recorded it closed → revive if terminal.
        (Some(IssueSyncState::Closed), Some(IssueSyncState::Open)) => {
            if task.status.is_terminal() {
                match store.transition(&task.id, Transition::Revive, ACTOR).await {
                    Ok(_) => tracing::info!(
                        task = %task.id, number,
                        "issue reopened on GitHub → task revived"
                    ),
                    Err(e) => tracing::info!(
                        task = %task.id,
                        "issue-reopen revive skipped ({e}); recording state only"
                    ),
                }
            }
            record_state(store, task, IssueSyncState::Open).await
        }
        // First sync of a state we hadn't recorded yet, or no change.
        (s, Some(live)) if s != Some(live) => record_state(store, task, live).await,
        _ => Ok(()),
    }
}

/// Persist a new `issue_synced_state` on the freshest copy of the task (the
/// transition above may have moved its lifecycle, so we re-read before writing).
async fn record_state(
    store: &StoreHandle,
    task: &Task,
    state: IssueSyncState,
) -> Result<(), IssueError> {
    let mut fresh = load(store, &task.id).await?;
    if fresh.issue_synced_state == Some(state) {
        return Ok(());
    }
    fresh.issue_synced_state = Some(state);
    store.set_issue_link(&fresh).await?;
    Ok(())
}

/// Every task carrying a linked issue (`issue_url.is_some()`).
async fn linked_tasks(store: &StoreHandle) -> Result<Vec<Task>, IssueError> {
    Ok(store
        .list_tasks(None)
        .await?
        .into_iter()
        .filter(|t| t.issue_url.is_some())
        .collect())
}

/// Parse a `#number`, a bare number, or a full issue URL into an issue number.
fn parse_link(link: &str) -> Option<u64> {
    let trimmed = link.trim();
    if let Some(rest) = trimmed.strip_prefix('#') {
        return rest.parse().ok();
    }
    if let Ok(n) = trimmed.parse::<u64>() {
        return Some(n);
    }
    issue_number_from_url(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_link_accepts_hash_bare_and_url() {
        assert_eq!(parse_link("#42"), Some(42));
        assert_eq!(parse_link("42"), Some(42));
        assert_eq!(parse_link("https://github.com/o/r/issues/7"), Some(7));
        assert_eq!(parse_link("https://github.com/o/r/issues/7/"), Some(7));
        assert_eq!(parse_link("not-a-link"), None);
    }
}
