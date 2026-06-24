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
//! ## The `http-fetch` grant (design §3.3)
//!
//! A guest holding the `http-fetch` capability — e.g. the `weather` extension
//! point — additionally gets outbound HTTP, but **only** through this one place
//! and **only** to an allowlisted set of hosts. [`HostState::with_http`] flips on
//! the `wasi:http` authority: the WASI context is built with IP name lookup +
//! outbound sockets enabled, and a [`HostAllowlist`] is enforced host-side in
//! [`AllowlistHooks::send_request`] before any connection is dialled. The empty
//! [`HostState::new`] state (used by the gate check / event reaction) links the
//! `wasi:http` interfaces too — so the import resolves — but with a deny-all
//! allowlist, so a guest that was *not* granted `http-fetch` can construct a
//! request and have it rejected, never reach the network.
//!
//! The richer capability set from design §3.3 (`store-read`, `secrets-read`,
//! `kv`, `emit-event`) attaches here too, gated on grants — those land in later
//! tasks. The shape (one place that owns every grant) is the point.

use wasmtime::StoreLimits;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::WasiHttpCtx;
use wasmtime_wasi_http::p2::body::HyperOutgoingBody;
use wasmtime_wasi_http::p2::types::{HostFutureIncomingResponse, OutgoingRequestConfig};
use wasmtime_wasi_http::p2::{
    HttpError, HttpResult, WasiHttpCtxView, WasiHttpHooks, WasiHttpView, default_send_request,
};

use crate::engine::EngineLimits;
use crate::http::HostAllowlist;

/// Per-store host state. One is created per invocation alongside its `Store`.
pub struct HostState {
    ctx: WasiCtx,
    http_ctx: WasiHttpCtx,
    table: ResourceTable,
    limits: StoreLimits,
    /// The `wasi:http` allowlist hooks. Always present (so the `wasi:http`
    /// import resolves for every guest); deny-all unless `http-fetch` was
    /// granted via [`HostState::with_http`].
    hooks: AllowlistHooks,
}

impl HostState {
    /// Build default-deny host state with the memory ceiling from `limits`.
    ///
    /// The WASI context is built empty: the guest is handed no preopens, no env,
    /// no inherited stdio, no sockets. That is the capability-based sandbox from
    /// design §2/§3.3 — authority is added explicitly, never inherited. Outbound
    /// HTTP is denied: the `wasi:http` interfaces link, but [`AllowlistHooks`]
    /// rejects every request (empty allowlist).
    pub fn new(limits: &EngineLimits) -> Self {
        // An empty builder grants nothing. We deliberately do NOT call
        // `inherit_stdio`/`inherit_env`/`preopened_dir`/`inherit_network`.
        let ctx = WasiCtxBuilder::new().build();
        Self::build(limits, ctx, HostAllowlist::new())
    }

    /// Build host state with the `http-fetch` grant: outbound HTTP enabled, but
    /// bounded to `allowlist`. Used for a `weather`-style guest that dials an
    /// upstream itself (design §3.3). The allowlist is the only thing between the
    /// guest and arbitrary exfiltration, so a deny-all `allowlist` here is still
    /// safe — the grant without hosts is a no-op.
    pub fn with_http(limits: &EngineLimits, allowlist: HostAllowlist) -> Self {
        // Enable outbound sockets + DNS so the `wasi:http` default sender can
        // actually dial. Still no FS / stdio / env — only the network authority
        // the `http-fetch` grant implies.
        let ctx = WasiCtxBuilder::new()
            .inherit_network()
            .allow_ip_name_lookup(true)
            .build();
        Self::build(limits, ctx, allowlist)
    }

    fn build(limits: &EngineLimits, ctx: WasiCtx, allowlist: HostAllowlist) -> Self {
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
            http_ctx: WasiHttpCtx::new(),
            table: ResourceTable::new(),
            limits: store_limits,
            hooks: AllowlistHooks {
                allowlist,
            },
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

// Gives the `wasi:http` host implementations access to the HTTP context, the
// resource table, and our allowlist hooks (design §3.3: the host owns the
// outbound decision, never the guest).
impl WasiHttpView for HostState {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http_ctx,
            table: &mut self.table,
            hooks: &mut self.hooks,
        }
    }
}

/// Host-side `wasi:http` hooks that enforce the [`HostAllowlist`] before any
/// outbound request is dialled (design §3.3). The guest can construct any
/// request it likes; this is where a request to a non-allowlisted host is
/// rejected, so the allowlist — not the guest — is authoritative.
struct AllowlistHooks {
    allowlist: HostAllowlist,
}

impl WasiHttpHooks for AllowlistHooks {
    fn send_request(
        &mut self,
        request: hyper::Request<HyperOutgoingBody>,
        config: OutgoingRequestConfig,
    ) -> HttpResult<HostFutureIncomingResponse> {
        // The bare host of the outbound URI; reject anything the allowlist does
        // not permit (an empty allowlist denies everything — default-deny).
        let host = request.uri().host().unwrap_or("");
        if !self.allowlist.is_allowed(host) {
            return Err(HttpError::trap(wasmtime::Error::msg(format!(
                "http-fetch denied: host `{host}` is not on the extension's allowlist"
            ))));
        }
        Ok(default_send_request(request, config))
    }
}
