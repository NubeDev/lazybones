//! `lazybones-ext` — the WASM extension runtime for lazybones.
//!
//! Hosts backend extensions as WebAssembly components (Wasmtime + the Component
//! Model / WASI Preview 2), with interfaces defined in WIT. See
//! `docs/design/extension-system.md` §2–§3 for the full rationale.
//!
//! This crate owns:
//! - the Wasmtime [`engine`] (shared, with fuel + epoch + memory limits),
//! - host-side [`caps`] (capability state guests run against, default-deny),
//! - typed [`fault`]s caught at the host/guest boundary,
//! - the [`gate`] extension-point host (P0: gate-check),
//! - the [`registry`] of installed extensions (indexed by exported WIT interface),
//! - extension-point [`dispatch`] (find-by-hook, invoke).
//!
//! It depends on `lazybones-store` for persistence; `lazybones-engine` and
//! `lazybones-api` will depend on it. WASM concerns stay out of the scheduler and
//! API crates.
//!
//! P0 status: the [`engine`], [`caps`], [`fault`], and [`gate`] modules are
//! implemented (the gate-check spike, design §5 P0). [`registry`] and the bulk of
//! [`dispatch`] are still scaffolds, wired in by later tasks.

pub mod caps;
pub mod dispatch;
pub mod engine;
pub mod fault;
pub mod gate;
pub mod registry;

pub use engine::{EngineLimits, ExtEngine};
pub use fault::ExtensionFault;
pub use gate::{DiffStat, GateCheckHost, GateInput, GateOutcome, Verdict, VerdictKind};
