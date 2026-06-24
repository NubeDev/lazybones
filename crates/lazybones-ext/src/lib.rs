//! `lazybones-ext` — the WASM extension runtime for lazybones.
//!
//! Hosts backend extensions as WebAssembly components (Wasmtime + the Component
//! Model / WASI Preview 2), with interfaces defined in WIT. See
//! `docs/design/extension-system.md` §2–§3 for the full rationale.
//!
//! This crate owns:
//! - the Wasmtime [`engine`] (shared, with fuel + epoch + memory limits),
//! - the [`registry`] of installed extensions (indexed by exported WIT interface),
//! - host-side [`caps`] (capability implementations guests may `import`,
//!   default-deny),
//! - extension-point [`dispatch`] (find-by-hook, invoke).
//!
//! It depends on `lazybones-store` for persistence; `lazybones-engine` and
//! `lazybones-api` will depend on it. WASM concerns stay out of the scheduler and
//! API crates.
//!
//! SCAFFOLD: modules are stubs. This crate is not yet wired into the API or
//! engine — that happens in a later task.

pub mod caps;
pub mod dispatch;
pub mod engine;
pub mod registry;
