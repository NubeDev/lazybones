//! Host capability state and the per-store data the guest runs against.
//!
//! Default-deny (design §3.3): a guest gets *nothing* unless the host explicitly
//! grants it. [`HostState`] is the `T` in `Store<T>`. For the P0 gate-check world
//! it carries:
//! - a minimal WASI Preview 2 context — built empty, so **no** FS, sockets, env,
//!   stdio, or clock-as-entropy is granted. WASI is wired into the linker only so
//!   the guest's language runtime (the Rust std on `wasm32-wasip2`) can link; it
//!   confers no actual ambient authority because the context grants nothing.
//! - the [`ResourceTable`] WASI resources live in.
//! - a [`StoreLimits`] enforcing the memory ceiling from [`EngineLimits`].
//!
//! The richer capability set from design §3.3 (`log`, `store-read`, `http-fetch`,
//! `secrets-read`, `kv`, `emit-event`) attaches here too, gated on grants — those
//! land in later tasks. The shape (one place that owns every grant) is the point.

use wasmtime::StoreLimits;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::engine::EngineLimits;

/// Per-store host state. One is created per invocation alongside its `Store`.
pub struct HostState {
    ctx: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
}

impl HostState {
    /// Build default-deny host state with the memory ceiling from `limits`.
    ///
    /// The WASI context is built empty: the guest is handed no preopens, no env,
    /// no inherited stdio, no sockets. That is the capability-based sandbox from
    /// design §2/§3.3 — authority is added explicitly, never inherited.
    pub fn new(limits: &EngineLimits) -> Self {
        // An empty builder grants nothing. We deliberately do NOT call
        // `inherit_stdio`/`inherit_env`/`preopened_dir`/`inherit_network`.
        let ctx = WasiCtxBuilder::new().build();

        // The memory ceiling is the load-bearing limit here. We deliberately do
        // NOT cap instance count: a single Component Model instantiation expands
        // into several internal core-wasm instances, so a low cap would reject
        // legitimate guests. CPU/wall-clock runaways are bounded by fuel + epoch,
        // not by instance count.
        let store_limits = wasmtime::StoreLimitsBuilder::new()
            .memory_size(limits.max_memory_bytes)
            .build();

        Self {
            ctx,
            table: ResourceTable::new(),
            limits: store_limits,
        }
    }

    /// Accessor for the store's memory/instance limiter, wired via `Store::limiter`.
    pub fn limits_mut(&mut self) -> &mut StoreLimits {
        &mut self.limits
    }
}

// Gives the WASI host implementations access to our context + resource table.
impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}
