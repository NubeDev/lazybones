//! Gate-check extension point host.
//!
//! Loads a `lazybones:ext/gate-check` guest component and invokes its `run`
//! export under the full resource regime from [`crate::engine`] (fuel + epoch +
//! memory + a host call timeout), catching every guest trap/panic at the boundary
//! and turning it into a typed [`ExtensionFault`] — design §3.2 / §3.4.
//!
//! Async scope is the conservative one: the guest runs via `call_async` (async
//! HOST functions), but the WIT itself is plain synchronous — no
//! `stream`/`future`/async exports.

use std::time::{Duration, Instant};

use wasmtime::Store;
use wasmtime::component::{Component, Linker};

use crate::caps::HostState;
use crate::engine::ExtEngine;
use crate::fault::ExtensionFault;

// Host-side bindings for our WIT world. `async: true` makes the generated
// `instantiate_async` / `call_run` futures, so guest execution yields to Tokio at
// epoch boundaries instead of blocking a worker (design §3.4: async host functions).
wasmtime::component::bindgen!({
    path: "wit",
    world: "extension",
    imports: { default: async },
    exports: { default: async },
});

// Re-export the generated guest types as the host's public gate-check vocabulary,
// so callers don't reach into the `exports::…` bindgen module path.
pub use exports::lazybones::ext::gate_check::{DiffStat, GateInput, Verdict, VerdictKind};

/// Outcome of one gate-check invocation: the guest's [`Verdict`] plus the measured
/// cold-instantiation latency for that call (design §3.4 calls this out as a
/// measured P0 input — it decides per-invocation vs pooled `Store`s).
#[derive(Debug, Clone)]
pub struct GateOutcome {
    /// The verdict the guest returned.
    pub verdict: Verdict,
    /// Time spent instantiating the component into a fresh store for this call.
    pub instantiation: Duration,
}

/// A loaded gate-check extension: a compiled [`Component`] plus the shared engine
/// and a WASI-populated linker. Compilation happens once at construction; each
/// [`evaluate`](Self::evaluate) creates a fresh, sandboxed `Store`.
pub struct GateCheckHost {
    engine: ExtEngine,
    component: Component,
    linker: Linker<HostState>,
}

impl GateCheckHost {
    /// Compile a gate-check component from raw `.wasm`/component bytes.
    pub fn from_bytes(engine: ExtEngine, bytes: &[u8]) -> Result<Self, ExtensionFault> {
        let component = Component::from_binary(engine.engine(), bytes)
            .map_err(|e| ExtensionFault::Load(e.to_string()))?;
        Self::from_component(engine, component)
    }

    /// Compile a gate-check component from a file on disk.
    pub fn from_file(
        engine: ExtEngine,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, ExtensionFault> {
        let component = Component::from_file(engine.engine(), path)
            .map_err(|e| ExtensionFault::Load(e.to_string()))?;
        Self::from_component(engine, component)
    }

    fn from_component(engine: ExtEngine, component: Component) -> Result<Self, ExtensionFault> {
        let mut linker = Linker::<HostState>::new(engine.engine());
        // WASI P2 host implementations, async flavour. This satisfies the imports
        // the guest's language runtime needs to link; it grants no ambient
        // authority on its own because each store's `WasiCtx` is built empty
        // (default-deny — see [`HostState`]).
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
        Ok(Self {
            engine,
            component,
            linker,
        })
    }

    /// Run the gate check against `input`.
    ///
    /// Every failure mode — guest trap/panic, fuel exhaustion, epoch deadline,
    /// memory limit, or the host call timeout — is caught and returned as a typed
    /// [`ExtensionFault`]. It never panics and never propagates a guest fault to
    /// the caller; mapping a fault to a fail-closed verdict is the dispatcher's job.
    pub async fn evaluate(&self, input: GateInput) -> Result<GateOutcome, ExtensionFault> {
        let limits = *self.engine.limits();

        let mut store = Store::new(self.engine.engine(), HostState::new(&limits));
        // Memory/instance ceiling.
        store.limiter(|state| state.limits_mut());
        // CPU bound.
        store
            .set_fuel(limits.fuel)
            .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
        // Wall-clock bound: trap (kill) when the epoch deadline lapses rather than
        // yielding — a runaway guest must die, not pause.
        store.set_epoch_deadline(limits.epoch_deadline_ticks());
        store.epoch_deadline_trap();

        let component = &self.component;
        let linker = &self.linker;

        let invocation = async move {
            let t0 = Instant::now();
            let instance = Extension::instantiate_async(&mut store, component, linker)
                .await
                .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
            let instantiation = t0.elapsed();

            let verdict = instance
                .lazybones_ext_gate_check()
                .call_run(&mut store, &input)
                .await
                .map_err(map_guest_error)?;

            Ok::<GateOutcome, ExtensionFault>(GateOutcome {
                verdict,
                instantiation,
            })
        };

        // Host-side last-resort timeout. Epoch interruption already bounds in-guest
        // CPU time; this also bounds a guest wedged awaiting host I/O. Give it a
        // little slack over the epoch budget so epoch/fuel traps win the race and
        // produce a more specific fault than a bare timeout.
        let budget = limits.wall_clock + Duration::from_secs(1);
        match tokio::time::timeout(budget, invocation).await {
            Ok(result) => result,
            Err(_elapsed) => Err(ExtensionFault::Timeout(budget)),
        }
    }
}

/// Map a Wasmtime error from guest execution onto a typed [`ExtensionFault`].
/// Resource kills (fuel/epoch) are distinguished from generic traps/panics so the
/// host can log *why* a guest died.
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
