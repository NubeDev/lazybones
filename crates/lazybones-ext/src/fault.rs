//! Typed extension faults.
//!
//! Everything that can go wrong while loading or invoking a guest is funnelled
//! into [`ExtensionFault`] at the host boundary. Guest traps, panics, fuel
//! exhaustion, epoch (wall-clock) deadline kills, OOM, and host-side call
//! timeouts are *caught here* and never propagate into a caller (design §3.4: a
//! looping/leaking guest is killed, the hook records a failure, the task is
//! unaffected — and a guest panic is "logged as an extension fault, never
//! propagate into the scheduler tick").
//!
//! Mapping a fault to a verdict is the *dispatcher's* policy, not this type's:
//! gate checks are fail-closed, so a fault becomes `fail`; event reactions are
//! fail-open. Keeping the fault typed (rather than collapsing straight to a
//! verdict) is what lets each extension point apply its own policy.

use std::time::Duration;

/// A fault raised at the host/guest boundary. Construction is host-internal; the
/// variants exist so callers can distinguish a *resource kill* (the guest misbehaved
/// and we stopped it) from a *load error* (the component was never runnable).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExtensionFault {
    /// The component bytes could not be read or compiled into a runnable artifact.
    #[error("failed to load extension component: {0}")]
    Load(String),

    /// The component compiled but could not be instantiated / its expected export
    /// was missing or mistyped.
    #[error("failed to instantiate extension component: {0}")]
    Instantiate(String),

    /// The guest exhausted its CPU fuel budget and was trapped. (`wasmtime::Trap::OutOfFuel`.)
    #[error("extension killed: CPU fuel budget exhausted")]
    FuelExhausted,

    /// The guest exceeded its wall-clock budget and was trapped by epoch
    /// interruption. (`wasmtime::Trap::Interrupt`.)
    #[error("extension killed: wall-clock (epoch) deadline exceeded")]
    Deadline,

    /// The host-side call timeout fired before the invocation returned — the
    /// last-resort net for a guest that is stuck awaiting host I/O rather than
    /// burning CPU/fuel.
    #[error("extension killed: host call timed out after {0:?}")]
    Timeout(Duration),

    /// The guest tried to grow memory past its limit and was denied.
    #[error("extension killed: memory limit exceeded")]
    MemoryLimit,

    /// The guest trapped or panicked for any other reason (unreachable, integer
    /// overflow, out-of-bounds, an explicit `panic!`, …). The string is the
    /// captured guest-side message/backtrace for logging.
    #[error("extension trapped: {0}")]
    Trap(String),
}

impl ExtensionFault {
    /// Whether this fault was the host deliberately killing a runaway guest
    /// (resource cap or timeout) as opposed to a load/instantiate problem. Used in
    /// tests and surfaced in logs.
    pub fn is_resource_kill(&self) -> bool {
        matches!(
            self,
            ExtensionFault::FuelExhausted
                | ExtensionFault::Deadline
                | ExtensionFault::Timeout(_)
                | ExtensionFault::MemoryLimit
        )
    }
}
