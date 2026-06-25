//! Content sync — the glue that turns the three building blocks (the store's
//! [`export_all`](lazybones_store::export_all)/[`import_all`](lazybones_store::import_all),
//! the [`SyncRepo`](lazybones_gh::SyncRepo) git transport, and the
//! [`Job`](lazybones_jobs::Job) service) into one feature the API and daemon
//! drive.
//!
//! Config lives in the operator's [`Preferences`](lazybones_store::Preferences)
//! (`sync` field). From it this module resolves a [`SyncRepo`] and offers three
//! operations:
//!
//! - [`status`] — fetch + compare local vs `origin/<branch>`, the read behind the
//!   "you're out of sync — pull?" banner.
//! - [`pull`] — `git pull` then [`import_all`] (catch up on a second machine).
//! - [`push`] — [`export_all`] then commit + push (before you leave a machine).
//!
//! [`pull`] and [`push`] are also wrapped as [`PullJob`]/[`PushJob`] so the
//! daemon can run them through the generic [`JobRunner`](lazybones_jobs::JobRunner)
//! (synchronously from an API request, or spawned in the background).

mod actions;
mod auto;
mod error;
mod job;
mod remote;
mod resolve;
mod status;

pub use actions::{ensure_repo, pull, push, PushOutcome};
pub use auto::{auto_pull_on_boot, auto_push_loop, spawn_auto_push, AUTO_PUSH_INTERVAL};
pub use error::SyncError;
pub use job::{PullJob, PushJob, PULL_JOB, PUSH_JOB};
pub use remote::{normalize_remote, provider_slug};
pub use status::{status, SyncState, SyncStatus};

/// The base sub-directory (under the data dir) the local sync checkout lives in
/// when the operator hasn't pinned an explicit `dir`. The actual checkout is
/// namespaced per provider beneath it — `<data_dir>/sync/<provider>` (e.g.
/// `sync/gh`) — so different remotes/auth never share a working tree.
pub(crate) const DEFAULT_SYNC_SUBDIR: &str = "sync";
