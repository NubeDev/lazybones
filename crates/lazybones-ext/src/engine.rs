//! Wasmtime engine ownership and per-invocation resource limits.
//!
//! One shared [`wasmtime::Engine`] across the process (compilation is expensive
//! and the engine is the thing worth sharing); `Store`s are instantiated
//! per-invocation with a fuel limit (CPU bound), epoch interruption (wall-clock
//! deadline), and a memory limiter — design §3.4.
//!
//! ## Async, but only host functions
//!
//! The engine is configured with `async_support(true)` so guests run via
//! `call_async` and outbound I/O can go through `wasi:http` without blocking a
//! Tokio worker. This is the *conservative* async scope from design §3.4: async
//! HOST functions only — we do **not** enable or use WIT-level async
//! (`stream`/`future`/async exports).
//!
//! ## How a runaway guest dies
//!
//! - **CPU fuel** (`consume_fuel`): a tight loop burns fuel and traps with
//!   `Trap::OutOfFuel` when the per-invocation budget is gone.
//! - **Epoch interruption** (`epoch_interruption`): a background ticker thread
//!   ([`EpochTicker`]) increments the engine epoch on a fixed cadence; each store
//!   gets a deadline in ticks and traps with `Trap::Interrupt` when it lapses.
//!   This catches loops that don't consume fuel (e.g. host-call spins) and bounds
//!   wall-clock regardless of fuel accounting.
//! - **Memory limiter**: enforced on the store via `StoreLimits` (see [`crate::caps`]).
//! - **Host call timeout**: a `tokio::time::timeout` around `call_async` is the
//!   last-resort net (see [`crate::gate`]).
//!
//! All four are independent: any one of them alone kills the guest, and the
//! resulting trap/timeout is caught at the host boundary as an
//! [`ExtensionFault`](crate::fault::ExtensionFault).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use wasmtime::{Config, Engine};

/// Per-invocation resource budget applied to every guest `Store`.
///
/// Defaults are deliberately tight — a gate check is cheap and rare, so generous
/// budgets would only widen the blast radius of a misbehaving guest.
#[derive(Debug, Clone, Copy)]
pub struct EngineLimits {
    /// CPU fuel granted per invocation. One unit is roughly one wasm bytecode
    /// operation, so this bounds total work, not time.
    pub fuel: u64,
    /// Wall-clock budget per invocation, enforced via epoch interruption.
    pub wall_clock: Duration,
    /// Maximum linear memory a guest instance may grow to, in bytes.
    pub max_memory_bytes: usize,
    /// Cadence of the epoch ticker. The wall-clock deadline is rounded up to a
    /// whole number of ticks, so this also sets the granularity of the deadline.
    pub epoch_tick: Duration,
}

impl Default for EngineLimits {
    fn default() -> Self {
        Self {
            // ~plenty for a verdict computation, nowhere near enough for a spin loop.
            fuel: 50_000_000,
            wall_clock: Duration::from_millis(500),
            // 64 MiB — comfortable for a small guest, a hard ceiling on leaks.
            max_memory_bytes: 64 * 1024 * 1024,
            epoch_tick: Duration::from_millis(5),
        }
    }
}

impl EngineLimits {
    /// A more generous profile for an extension point that does a real network
    /// round-trip (the `http-fetch` grant, e.g. the `weather` point). A gate
    /// check is pure compute and rare, so [`Default`] bounds it tight; an
    /// outbound fetch must survive DNS + TLS + an upstream reply, so the
    /// wall-clock budget is seconds, not milliseconds. CPU fuel and memory are
    /// also raised since a TLS handshake + JSON parse runs far more bytecode than
    /// a verdict computation.
    #[must_use]
    pub fn network() -> Self {
        Self {
            fuel: 5_000_000_000,
            wall_clock: Duration::from_secs(8),
            max_memory_bytes: 128 * 1024 * 1024,
            epoch_tick: Duration::from_millis(5),
        }
    }

    /// Number of epoch ticks corresponding to [`Self::wall_clock`] (rounded up,
    /// minimum one). This is what gets handed to `Store::set_epoch_deadline`.
    pub fn epoch_deadline_ticks(&self) -> u64 {
        let tick = self.epoch_tick.as_nanos().max(1);
        let budget = self.wall_clock.as_nanos();
        (budget.div_ceil(tick)).max(1) as u64
    }
}

/// The shared extension engine: one [`Engine`] plus the background epoch ticker
/// and the limit policy. Cheap to [`Clone`] (everything is behind an `Arc`), so
/// it can be handed to every extension point that needs to run guests.
#[derive(Clone)]
pub struct ExtEngine {
    engine: Engine,
    limits: EngineLimits,
    _ticker: Arc<EpochTicker>,
}

impl ExtEngine {
    /// Build the shared engine with the given limits, starting the epoch ticker.
    pub fn new(limits: EngineLimits) -> Result<Self, wasmtime::Error> {
        let mut config = Config::new();
        // Async host functions only (design §3.4): components run via call_async.
        // (As of Wasmtime 46 async support is always compiled in — there is no
        // longer a `Config::async_support` toggle — but the generated bindings are
        // still async via `imports/exports: { default: async }` in `gate.rs`, and
        // guests are driven with `*_async` entry points.)
        // Runaway protection.
        config.consume_fuel(true);
        config.epoch_interruption(true);
        // Component Model / WASI P2.
        config.wasm_component_model(true);

        let engine = Engine::new(&config)?;
        let ticker = EpochTicker::start(engine.clone(), limits.epoch_tick);

        Ok(Self {
            engine,
            limits,
            _ticker: Arc::new(ticker),
        })
    }

    /// The shared [`Engine`] — used to compile components and create stores.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The configured per-invocation limits.
    pub fn limits(&self) -> &EngineLimits {
        &self.limits
    }
}

/// Background thread that increments the engine epoch on a fixed cadence so that
/// per-store epoch deadlines actually advance. Stops and joins on drop.
struct EpochTicker {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl EpochTicker {
    fn start(engine: Engine, tick: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        // A plain std thread (not a Tokio task): epoch bumping must keep happening
        // even if every Tokio worker is parked, and it must not depend on any
        // particular runtime being live.
        let handle = std::thread::Builder::new()
            .name("lazybones-ext-epoch".into())
            .spawn(move || {
                while !stop_thread.load(Ordering::Relaxed) {
                    std::thread::sleep(tick);
                    engine.increment_epoch();
                }
            })
            .expect("spawn epoch ticker thread");

        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
