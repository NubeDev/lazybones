//! The pull and push operations — the two sides of content sync.

use std::path::Path;

use lazybones_gh::{Pushed, SyncRepo};
use lazybones_store::{BlobStore, SyncReport, StoreHandle, export_all, import_all};
use serde::Serialize;

use super::error::SyncError;
use super::resolve::{resolve, Resolved};

/// The result of a [`push`]: whether anything was sent, and what was exported.
#[derive(Debug, Clone, Serialize)]
pub struct PushOutcome {
    /// `true` if a commit was made and pushed; `false` if the export changed
    /// nothing (a clean no-op).
    pub pushed: bool,
    /// Per-kind counts of what was written to the export tree.
    #[serde(flatten)]
    pub exported: ReportCounts,
}

/// Per-kind counts, flattened into responses. Mirror of
/// [`SyncReport`](lazybones_store::SyncReport) with `Serialize` for the API.
#[derive(Debug, Clone, Serialize)]
pub struct ReportCounts {
    /// Documents exported/imported.
    pub documents: usize,
    /// Skills exported/imported.
    pub skills: usize,
    /// Tasks exported/imported.
    pub tasks: usize,
    /// Templates exported/imported.
    pub templates: usize,
    /// Workflows exported/imported.
    pub workflows: usize,
    /// Assets (images) exported/imported — metadata + bytes.
    pub assets: usize,
    /// Total records touched.
    pub total: usize,
}

impl From<SyncReport> for ReportCounts {
    fn from(r: SyncReport) -> Self {
        Self {
            documents: r.documents,
            skills: r.skills,
            tasks: r.tasks,
            templates: r.templates,
            workflows: r.workflows,
            assets: r.assets,
            total: r.total(),
        }
    }
}

/// Make sure the local checkout exists, cloning it from the remote if not. For a
/// brand-new (empty) remote where `clone --branch` can't find the branch yet,
/// fall back to `init` + `set_remote` so the first push can seed it.
///
/// # Errors
/// Returns [`SyncError`] if the repo can't be cloned/initialised.
pub async fn ensure_repo(store: &StoreHandle, data_dir: &Path) -> Result<SyncRepo, SyncError> {
    let resolved = resolve(store, data_dir).await?;
    checked_out(resolved).await
}

/// Pull the remote and import its contents into the store (the "catch up on a
/// second machine" path). Returns the per-kind import counts.
///
/// # Errors
/// Returns [`SyncError`] if sync is unconfigured, the pull fails (incl. a
/// non-fast-forward divergence), or the import fails.
pub async fn pull(
    store: &StoreHandle,
    blobs: &dyn BlobStore,
    data_dir: &Path,
) -> Result<ReportCounts, SyncError> {
    let resolved = resolve(store, data_dir).await?;
    let repo = checked_out(resolved).await?;
    repo.pull().await?;
    let report = import_all(store, blobs, repo.dir()).await?;
    Ok(report.into())
}

/// Export the store to the checkout, then commit + push (the "before you leave"
/// path). Returns whether anything was pushed and what was exported.
///
/// # Errors
/// Returns [`SyncError`] if sync is unconfigured, the export fails, or the
/// commit/push fails.
pub async fn push(
    store: &StoreHandle,
    blobs: &dyn BlobStore,
    data_dir: &Path,
) -> Result<PushOutcome, SyncError> {
    let resolved = resolve(store, data_dir).await?;
    let repo = checked_out(resolved).await?;
    let exported = export_all(store, blobs, repo.dir()).await?;
    let message = format!("lazybones sync {}", store.now());
    let pushed = matches!(repo.commit_and_push(&message).await?, Pushed::Committed);
    Ok(PushOutcome {
        pushed,
        exported: exported.into(),
    })
}

/// Open the resolved repo, auto-cloning it on first use. Always makes `gh`'s
/// token usable for https git first (so no SSH keys are needed), and reconciles
/// the `origin` URL with the (normalized) config — which repairs a checkout left
/// pointing at a stale/SSH remote by an earlier run.
async fn checked_out(resolved: Resolved) -> Result<SyncRepo, SyncError> {
    let remote = resolved
        .cfg
        .remote
        .as_deref()
        .ok_or(SyncError::Unconfigured)?;
    let dir = resolved.repo.dir().to_path_buf();
    let branch = resolved.repo.branch().to_owned();

    // Make git authenticate https with the gh token (idempotent, best-effort: a
    // missing gh shouldn't hard-fail before we even try to clone).
    if let Err(e) = resolved.repo.setup_git_auth().await {
        tracing::debug!("gh auth setup-git skipped: {e}");
    }

    if resolved.repo.is_checked_out() {
        // Keep origin in lock-step with the config so a changed URL (or a stale
        // SSH remote from before normalization) is corrected in place.
        if let Err(e) = resolved.repo.set_remote(remote).await {
            tracing::warn!("could not reconcile sync remote: {e}");
        }
        return Ok(resolved.repo);
    }

    // First use: auto-clone into the (provider-namespaced) dir. On an empty remote
    // (no branch yet) seed it with init + remote so the first push creates the branch.
    match SyncRepo::clone(remote, &dir, &branch).await {
        Ok(repo) => {
            tracing::info!(dir = %dir.display(), "cloned sync repo");
            Ok(repo)
        }
        Err(e) => {
            tracing::info!("sync clone failed ({e}); initialising a fresh checkout to seed the remote");
            let repo = SyncRepo::init(&dir, &branch).await?;
            repo.set_remote(remote).await?;
            Ok(repo)
        }
    }
}
