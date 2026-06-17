//! Embedded SurrealDB store boundary for lazybones — the durable brain of a run.
//!
//! The single source of truth (SCOPE.md principle 6): task documents, the
//! `depends_on` graph that drives readiness, and the `event` run log. Everything
//! a run needs to survive a restart lives here, reached over the [`StoreHandle`].

mod bootstrap;
mod check_health;
mod connect;
mod error;
mod event;
mod handle;
mod init_schema;
mod task;
mod workfile;

pub use connect::StoreEngine;
pub use error::{Result, StoreError};
pub use event::Event;
pub use handle::StoreHandle;
pub use task::{Status, Task, Transition};
pub use workfile::{SeedTask, sync_seeds};
