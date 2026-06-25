//! Errors from content-sync operations.

use lazybones_gh::GhError;
use lazybones_store::StoreError;

/// Failures from the content-sync glue. Kept distinct from the store/gh errors it
/// wraps so the API can map "not configured" to a clean 409 (an actionable
/// "set it up in Settings") rather than a generic 500.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SyncError {
    /// No sync remote is configured in preferences — the operator must set one up
    /// before any sync action can run.
    #[error("content sync is not configured (set a remote in Settings)")]
    Unconfigured,

    /// A git transport step (clone, fetch, pull, push) failed.
    #[error(transparent)]
    Gh(#[from] GhError),

    /// A store read/write (reading prefs, export, import) failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}
