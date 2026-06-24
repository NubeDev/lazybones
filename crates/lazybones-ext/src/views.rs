//! Projected, read-only store views — the `store-read` facade (design §3.7).
//!
//! `store-read` is a **versioned public contract**, not a window onto the
//! database. Guests never see a SurrealDB row, nor even the full domain model:
//! they see purpose-built projections ([`ExtTaskView`], [`ExtRunView`]) carrying
//! only the fields a gate check / event reaction actually needs. The projection
//! mapping lives here and *here only*, so internal model refactors stay free as
//! long as this mapping is updated — the guest contract is untouched (design
//! §3.7: "Project, don't expose").
//!
//! v1 is **read-only**: there is no projection back into a mutation. Write-back
//! happens through typed extension-point return values (a gate verdict, an
//! emitted event), never arbitrary store writes (`store-write` is deferred —
//! design §3.7).
//!
//! These are the Rust shapes of the WIT `ext-task-view` / `ext-run-view` records.
//! Wiring them as actual guest imports on the linker lands with the facade WIT
//! interface in a later task; the projection (the load-bearing, refactor-pinning
//! part) is defined now.

use lazybones_store::{Run, Task};

/// The semver of the `store-read` facade itself, versioned **independently** of
/// the WIT world (design §3.7: `lazybones:store-view@1.x`). Adding a projected
/// field is a minor bump; removing/retyping one is a major bump with a
/// deprecation window.
pub const STORE_VIEW_VERSION: &str = "1.0.0";

/// A read-only projection of a [`Task`] handed to a guest (design §3.7
/// `ext-task-view`). A deliberately small subset — never the full task row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtTaskView {
    /// The task's friendly id (unique within its run).
    pub id: String,
    /// The run this task belongs to.
    pub run: String,
    /// Human title.
    pub title: String,
    /// Current lifecycle status, as its wire string (`pending`/`ready`/…).
    pub status: String,
    /// Ids of the tasks this one depends on.
    pub deps: Vec<String>,
    /// The block reason, if the task is blocked.
    pub reason: Option<String>,
    /// The commit recorded on `done`, if any.
    pub commit: Option<String>,
}

impl ExtTaskView {
    /// Project the internal [`Task`] model into the guest-facing view. This is the
    /// single mapping point §3.7 depends on; the guest never sees fields not named
    /// here (worktree paths, sessions, retry policy, issue links, … stay host-only).
    #[must_use]
    pub fn project(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            run: task.run.clone(),
            title: task.title.clone(),
            status: task.status.as_str().to_owned(),
            deps: task.deps.clone(),
            reason: task.reason.clone(),
            commit: task.commit.clone(),
        }
    }
}

/// A read-only projection of a [`Run`] handed to a guest (design §3.7
/// `ext-run-view`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtRunView {
    /// The run's friendly id.
    pub id: String,
    /// Human title.
    pub title: String,
    /// Human-set lifecycle, as its wire string.
    pub lifecycle: String,
    /// RFC3339 creation timestamp.
    pub created_at: String,
}

impl ExtRunView {
    /// Project the internal [`Run`] model into the guest-facing view.
    #[must_use]
    pub fn project(run: &Run) -> Self {
        Self {
            id: run.id.clone(),
            title: run.title.clone(),
            lifecycle: run.lifecycle.as_str().to_owned(),
            created_at: run.created_at.clone(),
        }
    }
}
