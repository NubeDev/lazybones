//! The capabilities a scoped session may exercise over the REST surface.
//!
//! lazybones runs locally and trusts its loop, but an agent session is handed a
//! scoped grant rather than blanket access: it can drive its own task's lifecycle
//! and write memory, not reconfigure the run. Capabilities are the unit the API
//! checks before a mutating route runs (SCOPE.md, "Scoped session + capability
//! grants").

/// A single thing a session is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Import a workfile (loop only).
    Sync,
    /// Claim a ready task into running.
    Claim,
    /// Heartbeat a running task.
    Heartbeat,
    /// Mark a gating task done.
    Done,
    /// Block a task.
    Block,
    /// Write a memory record.
    Memorize,
    /// Read tasks, runs, and memory.
    Read,
    /// Manage agent CLI credentials (store/list/delete/export). Loop only — an
    /// agent session never reads or writes the secret store.
    Secret,
    /// Create, edit, or delete task records directly (author the queue in the DB; loop only).
    Author,
}

impl Capability {
    /// The full set the trusted loop holds.
    #[must_use]
    pub fn loop_grant() -> &'static [Capability] {
        &[
            Capability::Sync,
            Capability::Claim,
            Capability::Heartbeat,
            Capability::Done,
            Capability::Block,
            Capability::Memorize,
            Capability::Read,
            Capability::Secret,
            Capability::Author,
        ]
    }

    /// The set an agent session holds: drive its task + remember, never `Sync`.
    #[must_use]
    pub fn agent_grant() -> &'static [Capability] {
        &[
            Capability::Heartbeat,
            Capability::Done,
            Capability::Block,
            Capability::Memorize,
            Capability::Read,
        ]
    }
}
