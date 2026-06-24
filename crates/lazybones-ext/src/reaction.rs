//! Event-reaction extension point host.
//!
//! Loads a `lazybones:ext/event-reaction` guest component and invokes its `react`
//! export under the same full resource regime as the gate host (fuel + epoch +
//! memory + a host call timeout), catching every guest trap/panic at the boundary
//! and turning it into a typed [`ExtensionFault`] — design §3.2 / §3.4.
//!
//! Unlike the gate host, event reactions are **fail-open**: the
//! [`Dispatcher`](crate::dispatch::Dispatcher) logs a fault and lets the event
//! proceed rather than blocking anything. The host still kills a runaway guest the
//! same way; only the *policy* on the resulting fault differs.
//!
//! Async scope is the conservative one (design §3.4): the guest runs via
//! `call_async` (async HOST functions), but the WIT itself is plain synchronous.

use std::time::Duration;

use wasmtime::Store;
use wasmtime::component::{Component, Linker};

use crate::caps::HostState;
use crate::engine::ExtEngine;
use crate::fault::ExtensionFault;

// Host-side bindings for the reactor world. Generated separately from the
// gate-check world's bindings (different `world`), so the two extension points
// stay independent — a guest implements one world or the other.
wasmtime::component::bindgen!({
    path: "wit",
    world: "reactor",
    imports: { default: async },
    exports: { default: async },
});

// Re-export the generated guest types as the host's public event-reaction
// vocabulary, so callers don't reach into the `exports::…` bindgen module path.
pub use exports::lazybones::ext::event_reaction::{ActionKind, ExtAction, ExtEvent};

/// A loaded event-reaction extension: a compiled [`Component`] plus the shared
/// engine and a WASI-populated linker. Compilation happens once at construction;
/// each [`react`](Self::react) creates a fresh, sandboxed `Store`.
pub struct ReactionHost {
    engine: ExtEngine,
    component: Component,
    linker: Linker<HostState>,
}

impl ReactionHost {
    /// Compile a reaction component from raw `.wasm`/component bytes.
    pub fn from_bytes(engine: ExtEngine, bytes: &[u8]) -> Result<Self, ExtensionFault> {
        let component = Component::from_binary(engine.engine(), bytes)
            .map_err(|e| ExtensionFault::Load(e.to_string()))?;
        Self::from_component(engine, component)
    }

    /// Compile a reaction component from a file on disk.
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
        // WASI P2 host implementations, async flavour — grants no ambient authority
        // on its own (each store's `WasiCtx` is built empty; default-deny).
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
        Ok(Self {
            engine,
            component,
            linker,
        })
    }

    /// Run the reaction against `event`, returning the guest's follow-up actions.
    ///
    /// Every failure mode — guest trap/panic, fuel exhaustion, epoch deadline,
    /// memory limit, or the host call timeout — is caught and returned as a typed
    /// [`ExtensionFault`]; it never panics and never propagates a guest fault to
    /// the caller. The dispatcher applies the fail-open policy to the result.
    pub async fn react(&self, event: ExtEvent) -> Result<Vec<ExtAction>, ExtensionFault> {
        let limits = *self.engine.limits();

        let mut store = Store::new(self.engine.engine(), HostState::new(&limits));
        store.limiter(|state| state.limits_mut());
        store
            .set_fuel(limits.fuel)
            .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;
        store.set_epoch_deadline(limits.epoch_deadline_ticks());
        store.epoch_deadline_trap();

        let component = &self.component;
        let linker = &self.linker;

        let invocation = async move {
            let instance = Reactor::instantiate_async(&mut store, component, linker)
                .await
                .map_err(|e| ExtensionFault::Instantiate(e.to_string()))?;

            let actions = instance
                .lazybones_ext_event_reaction()
                .call_react(&mut store, &event)
                .await
                .map_err(map_guest_error)?;

            Ok::<Vec<ExtAction>, ExtensionFault>(actions)
        };

        // Host-side last-resort timeout (epoch/fuel traps win the race for a more
        // specific fault; see the gate host for the slack rationale).
        let budget = limits.wall_clock + Duration::from_secs(1);
        match tokio::time::timeout(budget, invocation).await {
            Ok(result) => result,
            Err(_elapsed) => Err(ExtensionFault::Timeout(budget)),
        }
    }
}

/// Map a Wasmtime error from guest execution onto a typed [`ExtensionFault`],
/// distinguishing resource kills (fuel/epoch) from generic traps/panics.
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
