//! Out-of-sync detection: fetch the remote and compare with the local checkout.

use std::path::Path;

use lazybones_store::StoreHandle;
use serde::Serialize;

use super::resolve::resolve;

/// Where the local checkout stands relative to `origin/<branch>` — what the UI
/// turns into a banner ("out of sync — pull?") or a quiet "in sync".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// No remote configured yet (operator hasn't set up sync).
    Unconfigured,
    /// Configured, but the local checkout doesn't exist yet (never pulled/cloned).
    NotCheckedOut,
    /// Local and remote match — nothing to do.
    Synced,
    /// Local has commits the remote doesn't — there's something to push.
    Ahead,
    /// Remote has commits the local doesn't — **offer a pull**.
    Behind,
    /// Both sides moved independently — needs a manual reconcile.
    Diverged,
    /// Couldn't determine (e.g. the fetch failed, offline) — don't nag.
    Unknown,
}

/// A full snapshot of the sync state for the API/UI. `state` is the headline; the
/// rest gives the settings page enough to show details.
#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    /// The headline state.
    pub state: SyncState,
    /// Commits the local checkout is ahead of the remote.
    pub ahead: u32,
    /// Commits the local checkout is behind the remote (drives the pull prompt).
    pub behind: u32,
    /// Whether the working tree has uncommitted changes (a half-done export).
    pub dirty: bool,
    /// The configured branch.
    pub branch: String,
    /// The configured remote URL, if any.
    pub remote: Option<String>,
}

impl SyncStatus {
    /// A terminal status (unconfigured / not-checked-out / unknown) with no
    /// counts — the cases where we can't or needn't compare commits.
    fn flat(state: SyncState, branch: String, remote: Option<String>) -> Self {
        Self {
            state,
            ahead: 0,
            behind: 0,
            dirty: false,
            branch,
            remote,
        }
    }
}

/// Compute the current [`SyncStatus`]. Never fails on a *network* problem — an
/// offline / unreachable remote resolves to [`SyncState::Unknown`] so the UI
/// quietly shows nothing rather than erroring. Only a store read failure
/// propagates.
///
/// # Errors
/// Returns a [`SyncError`](super::SyncError) only if reading preferences fails.
/// Being [`Unconfigured`](super::SyncError::Unconfigured) is reported *in* the
/// status (as [`SyncState::Unconfigured`]), not as an error.
pub async fn status(store: &StoreHandle, data_dir: &Path) -> Result<SyncStatus, super::SyncError> {
    let resolved = match resolve(store, data_dir).await {
        Ok(r) => r,
        Err(super::SyncError::Unconfigured) => {
            return Ok(SyncStatus::flat(SyncState::Unconfigured, "main".into(), None));
        }
        Err(e) => return Err(e),
    };
    let branch = resolved.cfg.branch_or_default().to_owned();
    let remote = resolved.cfg.remote.clone();

    if !resolved.repo.is_checked_out() {
        return Ok(SyncStatus::flat(SyncState::NotCheckedOut, branch, remote));
    }

    // Network step — failure here is "offline / can't tell", not an error.
    if resolved.repo.fetch().await.is_err() {
        return Ok(SyncStatus::flat(SyncState::Unknown, branch, remote));
    }

    let (ahead, behind) = resolved.repo.ahead_behind().await.unwrap_or((0, 0));
    let dirty = resolved.repo.is_dirty().await.unwrap_or(false);
    let state = match (ahead, behind) {
        (0, 0) => SyncState::Synced,
        (_, 0) => SyncState::Ahead,
        (0, _) => SyncState::Behind,
        (_, _) => SyncState::Diverged,
    };
    Ok(SyncStatus {
        state,
        ahead,
        behind,
        dirty,
        branch,
        remote,
    })
}
