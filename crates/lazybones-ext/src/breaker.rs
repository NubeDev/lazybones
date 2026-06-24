//! Per-extension circuit breaker (design §3.4).
//!
//! Per-call fail-open/closed policy handles *one* bad invocation. It does **not**
//! save us from a guest that is *consistently* wrong — a guest that faults on
//! every call, or (at a fail-closed point) rejects on every call. The design
//! makes this a first-class P1 concern, not a later discovery:
//!
//! > each extension carries a breaker: **N consecutive faults (or rejections at a
//! > fail-closed point) → auto-disable + surfaced alert.**
//!
//! This module owns that counter. It is the pure, thread-safe state machine; the
//! *consequence* of a trip (flipping the registry's `enabled` flag and surfacing
//! the alert) is the [`crate::dispatch::Dispatcher`]'s job, so the breaker stays a
//! testable leaf with no dependency on the registry.
//!
//! ## What counts toward a trip
//!
//! - A **fault** ([`record_fault`](CircuitBreaker::record_fault)) — a host-boundary
//!   [`ExtensionFault`](crate::ExtensionFault): trap, panic, fuel/epoch kill,
//!   timeout, OOM, or a load/instantiate failure. Always counts, at every
//!   extension point.
//! - A **rejection** ([`record_rejection`](CircuitBreaker::record_rejection)) — a
//!   guest that ran cleanly but *deterministically refuses* at a fail-closed point
//!   (e.g. a task-mutator that rejects every task). Counts only where the point's
//!   policy treats a steady stream of rejections as a malfunction (design §3.4's
//!   "rejections at a fail-closed point"). A gate-check `fail` is the gate doing
//!   its job, so the gate-check dispatcher deliberately does **not** feed
//!   rejections here — only faults — to avoid auto-disabling a security gate that
//!   is correctly blocking bad branches (see [`crate::dispatch`]).
//!
//! A clean, accepted call ([`record_success`](CircuitBreaker::record_success))
//! resets the consecutive counter to zero: the breaker trips only on an
//! *uninterrupted* run of failures.

use std::collections::HashMap;
use std::sync::Mutex;

/// The default consecutive-failure threshold at which an extension trips.
pub const DEFAULT_THRESHOLD: u32 = 5;

/// An alert surfaced when an extension's breaker trips (design §3.4: auto-disable
/// plus a surfaced alert). The [`Dispatcher`](crate::dispatch::Dispatcher) hands
/// this to its alert sink and disables the extension in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakerAlert {
    /// The extension id whose breaker tripped.
    pub ext_id: String,
    /// The consecutive-failure count that crossed the threshold.
    pub consecutive: u32,
    /// The threshold that was crossed.
    pub threshold: u32,
    /// A short reason for the last failure that tripped it (the fault/rejection
    /// message), for the surfaced alert.
    pub last_reason: String,
}

impl std::fmt::Display for BreakerAlert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "extension `{}` auto-disabled: {} consecutive failures (threshold {}) — last: {}",
            self.ext_id, self.consecutive, self.threshold, self.last_reason
        )
    }
}

/// Per-extension breaker state.
#[derive(Debug, Default, Clone, Copy)]
struct State {
    /// Consecutive failures since the last success (or since install).
    consecutive: u32,
    /// Whether this extension's breaker has already tripped. Once tripped it stays
    /// tripped until [`reset`](CircuitBreaker::reset) — a re-enable by an operator.
    tripped: bool,
}

/// A thread-safe registry of per-extension consecutive-failure counters.
///
/// Cloneable-by-`Arc` at the [`Dispatcher`](crate::dispatch::Dispatcher) level;
/// internally guarded by a `Mutex` so concurrent dispatches (the fail-open event
/// path can run several reactions at once) update it safely.
#[derive(Debug)]
pub struct CircuitBreaker {
    threshold: u32,
    state: Mutex<HashMap<String, State>>,
}

impl CircuitBreaker {
    /// A breaker tripping at `threshold` consecutive failures. A `threshold` of 0
    /// is clamped to 1 (a single failure trips), since a 0-threshold breaker would
    /// trip every extension on install.
    #[must_use]
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1),
            state: Mutex::new(HashMap::new()),
        }
    }

    /// The configured trip threshold.
    #[must_use]
    pub fn threshold(&self) -> u32 {
        self.threshold
    }

    /// Record a host-boundary **fault** for `ext_id`. Returns `Some(alert)` exactly
    /// on the call that trips the breaker (the threshold is crossed), and `None`
    /// otherwise — including on every subsequent fault once already tripped, so a
    /// caller fires the alert + auto-disable once, not on a loop.
    pub fn record_fault(&self, ext_id: &str, reason: &str) -> Option<BreakerAlert> {
        self.record_failure(ext_id, reason)
    }

    /// Record a clean-but-rejecting **rejection** for `ext_id` at a fail-closed
    /// point whose policy counts rejections (design §3.4). Same trip semantics as
    /// [`record_fault`](Self::record_fault).
    pub fn record_rejection(&self, ext_id: &str, reason: &str) -> Option<BreakerAlert> {
        self.record_failure(ext_id, reason)
    }

    fn record_failure(&self, ext_id: &str, reason: &str) -> Option<BreakerAlert> {
        let mut map = self.state.lock().expect("breaker state poisoned");
        let st = map.entry(ext_id.to_owned()).or_default();
        st.consecutive = st.consecutive.saturating_add(1);
        // Trip exactly once, on the call that crosses the threshold.
        if !st.tripped && st.consecutive >= self.threshold {
            st.tripped = true;
            return Some(BreakerAlert {
                ext_id: ext_id.to_owned(),
                consecutive: st.consecutive,
                threshold: self.threshold,
                last_reason: reason.to_owned(),
            });
        }
        None
    }

    /// Record a clean, accepted invocation for `ext_id`: resets the consecutive
    /// counter so the breaker only ever trips on an *uninterrupted* failure run.
    /// Does not un-trip an already-tripped breaker (that needs an operator re-enable
    /// via [`reset`](Self::reset)).
    pub fn record_success(&self, ext_id: &str) {
        let mut map = self.state.lock().expect("breaker state poisoned");
        if let Some(st) = map.get_mut(ext_id) {
            st.consecutive = 0;
        }
    }

    /// Whether `ext_id`'s breaker has tripped.
    #[must_use]
    pub fn is_tripped(&self, ext_id: &str) -> bool {
        self.state
            .lock()
            .expect("breaker state poisoned")
            .get(ext_id)
            .is_some_and(|s| s.tripped)
    }

    /// The current consecutive-failure count for `ext_id` (0 if unseen).
    #[must_use]
    pub fn consecutive(&self, ext_id: &str) -> u32 {
        self.state
            .lock()
            .expect("breaker state poisoned")
            .get(ext_id)
            .map_or(0, |s| s.consecutive)
    }

    /// Clear all breaker state for `ext_id` — used when an operator re-enables a
    /// previously auto-disabled extension, so it gets a fresh run of attempts
    /// rather than being one fault from re-tripping.
    pub fn reset(&self, ext_id: &str) {
        self.state
            .lock()
            .expect("breaker state poisoned")
            .remove(ext_id);
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(DEFAULT_THRESHOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trips_after_n_consecutive_faults_and_only_once() {
        let b = CircuitBreaker::new(3);
        assert!(b.record_fault("ext", "boom").is_none());
        assert!(b.record_fault("ext", "boom").is_none());
        // Third consecutive fault crosses the threshold → alert, exactly once.
        let alert = b.record_fault("ext", "boom").expect("should trip");
        assert_eq!(alert.consecutive, 3);
        assert_eq!(alert.threshold, 3);
        assert!(b.is_tripped("ext"));
        // Further faults don't re-alert (caller disables once).
        assert!(b.record_fault("ext", "boom").is_none());
        assert!(b.record_fault("ext", "boom").is_none());
    }

    #[test]
    fn success_resets_the_consecutive_run() {
        let b = CircuitBreaker::new(3);
        b.record_fault("ext", "x");
        b.record_fault("ext", "x");
        assert_eq!(b.consecutive("ext"), 2);
        b.record_success("ext");
        assert_eq!(b.consecutive("ext"), 0);
        // The run restarts; two more faults do NOT trip (would need three).
        b.record_fault("ext", "x");
        assert!(b.record_fault("ext", "x").is_none());
        assert!(!b.is_tripped("ext"));
    }

    #[test]
    fn rejections_count_toward_the_same_breaker() {
        let b = CircuitBreaker::new(2);
        assert!(b.record_rejection("mut", "rejected").is_none());
        let alert = b.record_rejection("mut", "rejected").expect("trips");
        assert_eq!(alert.ext_id, "mut");
        assert!(b.is_tripped("mut"));
    }

    #[test]
    fn breakers_are_independent_per_extension() {
        let b = CircuitBreaker::new(2);
        b.record_fault("a", "x");
        b.record_fault("a", "x");
        assert!(b.is_tripped("a"));
        // `b`'s breaker is untouched.
        assert!(!b.is_tripped("b"));
        assert_eq!(b.consecutive("b"), 0);
    }

    #[test]
    fn reset_clears_a_tripped_breaker() {
        let b = CircuitBreaker::new(1);
        assert!(b.record_fault("ext", "x").is_some());
        assert!(b.is_tripped("ext"));
        b.reset("ext");
        assert!(!b.is_tripped("ext"));
        assert_eq!(b.consecutive("ext"), 0);
    }

    #[test]
    fn zero_threshold_is_clamped_to_one() {
        let b = CircuitBreaker::new(0);
        assert_eq!(b.threshold(), 1);
        assert!(b.record_fault("ext", "x").is_some());
    }
}
