//! `lazybones-ext` — the WASM extension runtime for lazybones.
//!
//! Hosts backend extensions as WebAssembly components (Wasmtime + the Component
//! Model / WASI Preview 2), with interfaces defined in WIT. See
//! `docs/design/extension-system.md` §2–§3 for the full rationale.
//!
//! This crate owns:
//! - the Wasmtime [`engine`] (shared, with fuel + epoch + memory limits),
//! - host-side [`caps`] (capability state guests run against, default-deny),
//! - the [`capability`] vocabulary + grant policy (`granted ⊆ requested`),
//! - the read-only projected store [`views`] (`ext-task-view` / `ext-run-view`)
//!   and the `http`-fetch host allowlist — the v1 host capabilities,
//! - the [`manifest`] (embedded `lazybones.ext.toml`) parser + custom-section reader,
//! - typed [`fault`]s caught at the host/guest boundary,
//! - the [`gate`] and [`reaction`] extension-point hosts (gate-check / event-reaction),
//! - the per-extension [`breaker`] (circuit breaker) and event [`cycle`] guard (§3.4),
//! - the [`registry`] of installed extensions (indexed by exported WIT interface),
//! - extension-point [`dispatch`] (the [`Dispatcher`]: find-by-hook, invoke, apply
//!   fail-open/closed policy + breaker + cycle guard).
//!
//! It depends on `lazybones-store` for persistence; `lazybones-engine` and
//! `lazybones-api` depend on it. WASM concerns stay out of the scheduler and API
//! crates — the scheduler holds only an [`Dispatcher`] handle.

pub mod breaker;
pub mod capability;
pub mod caps;
pub mod cycle;
pub mod dispatch;
pub mod engine;
pub mod fault;
pub mod gate;
pub mod http;
pub mod manifest;
pub mod reaction;
pub mod registry;
pub mod views;

pub use breaker::{BreakerAlert, CircuitBreaker};
pub use capability::{Capability, GrantError, validate_grant};
pub use cycle::{Admission, CycleConfig, CycleGuard, EventOrigin};
pub use dispatch::{
    AlertSink, ComponentLoader, Dispatcher, DispatcherConfig, GateDecision, HostEvent,
    LogAlertSink, EVENT_REACTION_INTERFACE, GATE_CHECK_INTERFACE,
};
pub use engine::{EngineLimits, ExtEngine};
pub use fault::ExtensionFault;
pub use gate::{DiffStat, GateCheckHost, GateInput, GateOutcome, Verdict, VerdictKind};
pub use http::HostAllowlist;
pub use manifest::{Manifest, ManifestError, MANIFEST_SECTION};
pub use reaction::{ActionKind, ExtAction, ExtEvent, ReactionHost};
pub use registry::{ExtensionRecord, InstallRequest, RecordClaims, Registry, RegistryError};
pub use views::{ExtRunView, ExtTaskView, STORE_VIEW_VERSION};
