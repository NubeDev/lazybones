//! Extension-point dispatch.
//!
//! Resolves which extensions export a given WIT interface and invokes them in
//! order, applying per-extension-point policy: fail-open vs fail-closed, the
//! per-extension circuit breaker, and the event cycle guard (origin tag + depth +
//! rate limit) — design §3.2, §3.4.
//!
//! SCAFFOLD: the registry-driven multi-extension dispatch loop lands in a later
//! task. What is implemented now is the one piece the P0 gate-check host needs:
//! the **fail-closed policy** that turns a host-boundary [`ExtensionFault`] into a
//! verdict (design §3.4: "gate checks fail-closed").

use crate::fault::ExtensionFault;
use crate::gate::{Verdict, VerdictKind};

/// Apply the gate-check fail-closed policy to one invocation result.
///
/// A successful guest verdict is taken as-is. Any host-boundary fault — trap,
/// panic, fuel/epoch kill, timeout, OOM — is mapped to a **`fail`** verdict (never
/// `skip`), so a misbehaving or absent gate blocks the land rather than waving it
/// through (design §3.4). The fault is preserved in the message for operator
/// surfacing.
pub fn gate_verdict_fail_closed(result: Result<Verdict, ExtensionFault>) -> Verdict {
    match result {
        Ok(verdict) => verdict,
        Err(fault) => Verdict {
            kind: VerdictKind::Fail,
            message: format!("gate check failed closed: {fault}"),
        },
    }
}
