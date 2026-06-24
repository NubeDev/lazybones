//! Event reentrancy / cycle guard (design §3.4).
//!
//! Event-reaction guests get both event *subscription* and (eventually)
//! `emit-event`. That is a feedback loop by construction: A emits → A wakes; or
//! B → A → B. This repo already carries a documented `auto_pr` infinite-spawn
//! flaw, so the design makes the guard **mandatory from P1** — cycles are designed
//! out, not found in production:
//!
//! > extension-emitted events carry an **origin tag + emission depth**; an event
//! > whose causal chain re-enters the same extension beyond a small depth is
//! > dropped, and every extension has a **per-window emit rate limit**.
//!
//! This module is the pure decision logic for all three:
//!
//! 1. **Origin tag + emission depth** — [`EventOrigin`] travels with every event.
//!    A host-origin event has `origin = None, depth = 0`. When extension `X` emits
//!    a follow-up, [`EventOrigin::descend`] stamps the child `origin = X,
//!    depth = parent + 1`.
//! 2. **Depth / reentry drop** — [`CycleGuard::admit`] refuses to dispatch an event
//!    to an extension when the chain is too deep, or when it would re-enter the
//!    same extension that emitted it beyond a small self-reentry bound.
//! 3. **Per-window emit rate limit** — [`CycleGuard::allow_emit`] caps how many
//!    events one extension may emit within a rolling window, the backstop against
//!    a guest that emits *distinct* events fast enough to dodge the depth guard.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The cycle guard's bounds. Kept deliberately small — the point is to make a
/// runaway loop impossible, not to support deep legitimate chains (design §3.4:
/// "beyond a *small* depth").
#[derive(Debug, Clone, Copy)]
pub struct CycleConfig {
    /// Hard ceiling on a causal chain's emission depth. An event at or beyond this
    /// depth is never dispatched to any reaction, full stop — the global backstop
    /// against an A→B→A→… chain growing without bound.
    pub max_depth: u32,
    /// How many times an event may re-enter the *same* extension that emitted it
    /// before being dropped. `1` means an extension never reacts to an event it
    /// itself emitted (the tightest A→A guard); a small value > 1 allows a short
    /// self-referential chain.
    pub max_self_reentry: u32,
    /// Maximum events one extension may emit within [`window`](Self::window).
    pub emits_per_window: u32,
    /// The rolling window the [`emits_per_window`](Self::emits_per_window) budget
    /// applies over.
    pub window: Duration,
}

impl Default for CycleConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_self_reentry: 1,
            emits_per_window: 20,
            window: Duration::from_secs(10),
        }
    }
}

/// The provenance an event carries through the cycle guard: which extension (if
/// any) emitted it, and how deep along the causal chain it sits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventOrigin {
    /// The extension id that emitted this event, or `None` for a host-origin event
    /// (a real lifecycle transition off the durable stream).
    pub origin: Option<String>,
    /// Emission depth: `0` for a host-origin event, `parent + 1` for each
    /// extension emission along the chain.
    pub depth: u32,
}

impl EventOrigin {
    /// A host-origin event — the root of any causal chain (`origin = None`,
    /// `depth = 0`).
    #[must_use]
    pub fn host() -> Self {
        Self {
            origin: None,
            depth: 0,
        }
    }

    /// Derive the origin of an event *emitted by* `ext_id` in reaction to `self`:
    /// the child carries `ext_id` as its origin and `self.depth + 1` as its depth.
    /// This is what stamps the "origin tag + emission depth" onto every
    /// extension-emitted event (design §3.4).
    #[must_use]
    pub fn descend(&self, ext_id: &str) -> Self {
        Self {
            origin: Some(ext_id.to_owned()),
            depth: self.depth.saturating_add(1),
        }
    }
}

/// The guard's verdict on whether an event may be dispatched to a reaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// Dispatch is allowed.
    Admit,
    /// Dropped: the causal chain is at/over [`CycleConfig::max_depth`].
    DropDepth {
        /// The event's depth that hit the ceiling.
        depth: u32,
    },
    /// Dropped: the event would re-enter the same extension that emitted it beyond
    /// [`CycleConfig::max_self_reentry`].
    DropReentry {
        /// The extension being re-entered.
        ext_id: String,
        /// The event's depth at the point of re-entry.
        depth: u32,
    },
}

impl Admission {
    /// Whether dispatch is allowed.
    #[must_use]
    pub fn is_admitted(&self) -> bool {
        matches!(self, Admission::Admit)
    }
}

/// Per-extension emit-rate state for the rolling window.
#[derive(Debug, Clone, Copy)]
struct RateState {
    window_start: Instant,
    count: u32,
}

/// The cycle guard: pure depth/reentry admission plus a per-extension emit-rate
/// limiter. Thread-safe (the fail-open event path may dispatch concurrently).
#[derive(Debug)]
pub struct CycleGuard {
    cfg: CycleConfig,
    rate: Mutex<HashMap<String, RateState>>,
}

impl CycleGuard {
    /// A guard with the given bounds.
    #[must_use]
    pub fn new(cfg: CycleConfig) -> Self {
        Self {
            cfg,
            rate: Mutex::new(HashMap::new()),
        }
    }

    /// The configured bounds.
    #[must_use]
    pub fn config(&self) -> &CycleConfig {
        &self.cfg
    }

    /// Decide whether an event with provenance `origin` may be dispatched to the
    /// reaction extension `target_ext`.
    ///
    /// Drops when the chain is too deep, or when dispatching would re-enter the
    /// extension that emitted the event beyond the self-reentry bound. Otherwise
    /// admits. This is the heart of the "no infinite spawn" guarantee.
    #[must_use]
    pub fn admit(&self, target_ext: &str, origin: &EventOrigin) -> Admission {
        if origin.depth >= self.cfg.max_depth {
            return Admission::DropDepth {
                depth: origin.depth,
            };
        }
        if origin.origin.as_deref() == Some(target_ext) && origin.depth >= self.cfg.max_self_reentry
        {
            return Admission::DropReentry {
                ext_id: target_ext.to_owned(),
                depth: origin.depth,
            };
        }
        Admission::Admit
    }

    /// Charge one emission against `ext_id`'s rolling-window budget, returning
    /// whether it is within the limit. Uses the wall clock for the window; call
    /// [`allow_emit_at`](Self::allow_emit_at) directly in tests to control time.
    pub fn allow_emit(&self, ext_id: &str) -> bool {
        self.allow_emit_at(ext_id, Instant::now())
    }

    /// [`allow_emit`](Self::allow_emit) with an explicit `now`, so the rolling
    /// window is testable without sleeping.
    pub fn allow_emit_at(&self, ext_id: &str, now: Instant) -> bool {
        let mut map = self.rate.lock().expect("cycle rate state poisoned");
        let st = map.entry(ext_id.to_owned()).or_insert(RateState {
            window_start: now,
            count: 0,
        });
        // Roll the window forward once it has fully elapsed.
        if now.duration_since(st.window_start) >= self.cfg.window {
            st.window_start = now;
            st.count = 0;
        }
        if st.count >= self.cfg.emits_per_window {
            return false;
        }
        st.count += 1;
        true
    }
}

impl Default for CycleGuard {
    fn default() -> Self {
        Self::new(CycleConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_event_descends_with_origin_and_incremented_depth() {
        let root = EventOrigin::host();
        assert_eq!(root.origin, None);
        assert_eq!(root.depth, 0);
        let child = root.descend("ext-a");
        assert_eq!(child.origin.as_deref(), Some("ext-a"));
        assert_eq!(child.depth, 1);
        let grandchild = child.descend("ext-b");
        assert_eq!(grandchild.depth, 2);
    }

    #[test]
    fn admits_a_host_event_to_any_extension() {
        let g = CycleGuard::new(CycleConfig::default());
        assert!(g.admit("ext-a", &EventOrigin::host()).is_admitted());
    }

    #[test]
    fn drops_when_chain_exceeds_max_depth() {
        let g = CycleGuard::new(CycleConfig {
            max_depth: 3,
            ..CycleConfig::default()
        });
        let deep = EventOrigin {
            origin: Some("ext-x".into()),
            depth: 3,
        };
        assert_eq!(g.admit("ext-y", &deep), Admission::DropDepth { depth: 3 });
    }

    #[test]
    fn drops_self_reentry_beyond_bound() {
        // max_self_reentry = 1: an extension never reacts to its own emission.
        let g = CycleGuard::new(CycleConfig {
            max_self_reentry: 1,
            ..CycleConfig::default()
        });
        let own = EventOrigin {
            origin: Some("ext-a".into()),
            depth: 1,
        };
        assert_eq!(
            g.admit("ext-a", &own),
            Admission::DropReentry {
                ext_id: "ext-a".into(),
                depth: 1
            }
        );
        // ...but a *different* extension may still react to it (B→A is fine).
        assert!(g.admit("ext-b", &own).is_admitted());
    }

    #[test]
    fn rate_limit_caps_emissions_per_window_and_rolls_over() {
        let g = CycleGuard::new(CycleConfig {
            emits_per_window: 2,
            window: Duration::from_secs(10),
            ..CycleConfig::default()
        });
        let t0 = Instant::now();
        assert!(g.allow_emit_at("ext", t0));
        assert!(g.allow_emit_at("ext", t0));
        // Third within the window is refused.
        assert!(!g.allow_emit_at("ext", t0));
        // After the window elapses the budget resets.
        let t1 = t0 + Duration::from_secs(11);
        assert!(g.allow_emit_at("ext", t1));
    }

    #[test]
    fn rate_limit_is_per_extension() {
        let g = CycleGuard::new(CycleConfig {
            emits_per_window: 1,
            ..CycleConfig::default()
        });
        let t = Instant::now();
        assert!(g.allow_emit_at("a", t));
        assert!(!g.allow_emit_at("a", t));
        // `b` has its own independent budget.
        assert!(g.allow_emit_at("b", t));
    }
}
