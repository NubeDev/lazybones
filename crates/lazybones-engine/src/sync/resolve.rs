//! Resolve the operator's [`SyncConfig`] into a ready-to-drive [`SyncRepo`].

use std::path::{Path, PathBuf};

use lazybones_gh::SyncRepo;
use lazybones_store::{StoreHandle, SyncConfig};

use super::remote::{normalize_remote, provider_slug};
use super::{DEFAULT_SYNC_SUBDIR, error::SyncError};

/// The resolved sync target: the configured values plus the local [`SyncRepo`]
/// pinned to the right dir + branch.
pub(crate) struct Resolved {
    /// The operator's config (remote, branch, auto-push/pull flags).
    pub(crate) cfg: SyncConfig,
    /// The git transport for the local checkout.
    pub(crate) repo: SyncRepo,
}

/// Read preferences and build a [`Resolved`] target, or [`SyncError::Unconfigured`]
/// if no remote is set. `data_dir` is the daemon's data directory, used to derive
/// the checkout path when the operator hasn't pinned an explicit `dir`.
pub(crate) async fn resolve(store: &StoreHandle, data_dir: &Path) -> Result<Resolved, SyncError> {
    let mut cfg = store
        .get_preferences()
        .await?
        .unwrap_or_default()
        .sync
        .unwrap_or_default();
    if !cfg.is_configured() {
        return Err(SyncError::Unconfigured);
    }

    // Canonicalise whatever the operator typed (shorthand / https / ssh) to a
    // gh-auth-friendly https URL, and store it back on the resolved config so the
    // clone/push/pull all use the working form.
    let remote = normalize_remote(cfg.remote.as_deref().unwrap_or_default());
    cfg.remote = Some(remote.clone());

    // Default the checkout to `<data_dir>/sync/<provider>` so different remotes
    // (with different auth) never collide in one working tree.
    let dir: PathBuf = cfg
        .dir
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            data_dir
                .join(DEFAULT_SYNC_SUBDIR)
                .join(provider_slug(&remote))
        });
    let repo = SyncRepo::open(dir, cfg.branch_or_default());
    Ok(Resolved { cfg, repo })
}
