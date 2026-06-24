//! Extension-point dispatch.
//!
//! Resolves which extensions export a given WIT interface and invokes them in
//! order, applying per-extension-point policy: fail-open vs fail-closed, the
//! per-extension circuit breaker, and the event cycle guard (origin tag + depth +
//! rate limit) — design §3.2, §3.4.
//!
//! SCAFFOLD: filled in by a later task.
