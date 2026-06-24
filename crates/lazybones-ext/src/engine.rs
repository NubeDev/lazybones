//! Wasmtime engine ownership.
//!
//! One shared [`wasmtime::Engine`] across the process; `Store`s are instantiated
//! per-invocation (or pooled) with a fuel limit (CPU bound), epoch interruption
//! (wall-clock deadline), and a memory limiter — see design §3.4. Guest
//! traps/panics are caught at the host boundary and never propagate into the
//! scheduler tick.
//!
//! SCAFFOLD: filled in by a later task.
