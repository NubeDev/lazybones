//! Weather extension-point host (the `http-fetch` example, design §3.2/§3.3).
//!
//! Loads a `lazybones:ext/weather` guest (the `weather-provider` world) and
//! invokes its `get-weather` export. Unlike the gate check, this guest does REAL
//! OUTBOUND WORK: the `weather-provider` world imports
//! `wasi:http/outgoing-handler`, so the guest dials a keyless weather API itself
//! and parses the response — the host does 100% of *granting* the capability and
//! 0% of the fetch.
//!
//! The outbound authority is bounded two ways (design §3.3):
//! - the per-store [`HostState::with_http`] only enables the network when the
//!   extension holds the `http-fetch` grant, and
//! - a [`HostAllowlist`] is enforced host-side (see [`crate::caps`]) so the guest
//!   can only reach the hosts the install allowed.
//!
//! Everything else mirrors [`crate::gate`]: the same shared engine, the same
//! fuel/epoch/memory/timeout regime (on the more generous [`EngineLimits::network`]
//! profile, since a TLS round-trip is not a 500ms compute), and every guest
//! trap/panic caught at the boundary as a typed [`ExtensionFault`]. Async scope
//! is the conservative one — async HOST functions, plain synchronous WIT.

use std::time::{Duration, Instant};

use wasmtime::Store;
use wasmtime::component::{Component, Linker};

use crate::caps::HostState;
use crate::engine::ExtEngine;
use crate::fault::ExtensionFault;
use crate::http::HostAllowlist;

// Host-side bindings for the `weather-provider` world. `async` so guest execution
// yields to Tokio at epoch boundaries and the imported `wasi:http` host functions
// run async (design §3.4: async host functions).
wasmtime::component::bindgen!({
    path: "wit",
    world: "weather-provider",
    imports: { default: async },
    exports: { default: async },
    // The world imports `wasi:http/outgoing-handler`; let bindgen reuse the
    // `wasmtime-wasi-http` host types for those interfaces instead of generating
    // its own, so `add_to_linker_async` satisfies the imports.
    with: {
        "wasi:http": wasmtime_wasi_http::p2::bindings::http,
        "wasi:io": wasmtime_wasi::p2::bindings::io,
        "wasi:clocks": wasmtime_wasi::p2::bindings::clocks,
    },
});

// Re-export the generated guest types as the host's public weather vocabulary.
pub use exports::lazybones::ext::weather::{WeatherQuery, WeatherResult};

/// Outcome of one weather invocation: the guest's [`WeatherResult`] plus the
/// measured cold-instantiation latency (design §3.4's measured P0 input).
#[derive(Debug, Clone)]
pub struct WeatherOutcome {
    /// The result the guest returned (may carry its own `error`).
    pub result: WeatherResult,
    /// Time spent instantiating the component into a fresh store for this call.
    pub instantiation: Duration,
}

/// A loaded weather extension: a compiled [`Component`], the shared engine, a
/// WASI + `wasi:http`-populated linker, and the host allowlist the guest's
/// outbound requests are bounded to.
pub struct WeatherHost {
    engine: ExtEngine,
    component: Component,
    linker: Linker<HostState>,
    allowlist: HostAllowlist,
}

impl WeatherHost {
    /// Compile a weather component from raw `.wasm`/component bytes, bounding its
    /// outbound HTTP to `allowlist` (design §3.3).
    pub fn from_bytes(
        engine: ExtEngine,
        bytes: &[u8],
        allowlist: HostAllowlist,
    ) -> Result<Self, ExtensionFault> {
        let component = Component::from_binary(engine.engine(), bytes)
            .map_err(|e| ExtensionFault::Load(e.to_string()))?;
        Self::from_component(engine, component, allowlist)
    }

    fn from_component(
        engine: ExtEngine,
        component: Component,
        allowlist: HostAllowlist,
    ) -> Result<Self, ExtensionFault> {
        let mut linker = Linker::<HostState>::new(engine.engine());
        // WASI P2 (the guest runtime's imports) + `wasi:http` (the outbound
        // handler the `weather-provider` world imports). Both are async host
        // functions. Neither confers ambient authority on its own: the per-store
        // `HostState` decides what the guest may actually reach.
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
        wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker)
            .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
        Ok(Self {
            engine,
            component,
            linker,
            allowlist,
        })
    }

    /// Run the weather lookup for `query`.
    ///
    /// Every failure mode — guest trap/panic, fuel exhaustion, epoch deadline,
    /// memory limit, the host call timeout, or a denied outbound host — is caught
    /// and returned as a typed [`ExtensionFault`]. It never panics. A guest that
    /// *runs* but could not resolve the place returns a [`WeatherResult`] with its
    /// `error` set instead — that is a clean return, not a fault.
    pub async fn fetch(&self, query: WeatherQuery) -> Result<WeatherOutcome, ExtensionFault> {
        // Network profile: a real round-trip needs seconds + more fuel/memory than
        // a gate verdict (design §3.4).
        let limits = crate::engine::EngineLimits::network();

        let mut store =
            Store::new(self.engine.engine(), HostState::with_http(&limits, self.allowlist.clone()));
        store.limiter(|state| state.limits_mut());
        store
            .set_fuel(limits.fuel)
            .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
        store.set_epoch_deadline(limits.epoch_deadline_ticks());
        store.epoch_deadline_trap();

        let component = &self.component;
        let linker = &self.linker;

        let invocation = async move {
            let t0 = Instant::now();
            let instance = WeatherProvider::instantiate_async(&mut store, component, linker)
                .await
                .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
            let instantiation = t0.elapsed();

            let result = instance
                .lazybones_ext_weather()
                .call_get_weather(&mut store, &query)
                .await
                .map_err(map_guest_error)?;

            Ok::<WeatherOutcome, ExtensionFault>(WeatherOutcome {
                result,
                instantiation,
            })
        };

        let budget = limits.wall_clock + Duration::from_secs(1);
        match tokio::time::timeout(budget, invocation).await {
            Ok(result) => result,
            Err(_elapsed) => Err(ExtensionFault::Timeout(budget)),
        }
    }
}

/// Map a Wasmtime error from guest execution onto a typed [`ExtensionFault`].
/// Resource kills (fuel/epoch) are distinguished from generic traps/panics.
fn map_guest_error(err: wasmtime::Error) -> ExtensionFault {
    if let Some(trap) = err.downcast_ref::<wasmtime::Trap>() {
        return match trap {
            wasmtime::Trap::OutOfFuel => ExtensionFault::FuelExhausted,
            wasmtime::Trap::Interrupt => ExtensionFault::Deadline,
            _ => ExtensionFault::Trap(format!("{err:?}")),
        };
    }
    ExtensionFault::Trap(format!("{err:?}"))
}
