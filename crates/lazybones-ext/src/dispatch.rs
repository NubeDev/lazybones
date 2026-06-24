//! Extension-point dispatch.
//!
//! Resolves which extensions export a given WIT interface and invokes them in
//! order, applying per-extension-point policy: **fail-open** vs **fail-closed**,
//! the per-extension **circuit breaker** ([`crate::breaker`]), and the event
//! **cycle guard** ([`crate::cycle`]) — design §3.2, §3.4.
//!
//! Two extension points are wired here, matching the seams the scheduler already
//! has:
//!
//! - **gate-check** ([`Dispatcher::run_gate_checks`]) — runs at the existing gate
//!   point, **FAIL-CLOSED**: any extension that returns `fail`, *or* faults at the
//!   host boundary (trap/fuel/epoch/OOM/timeout), blocks the land. An absent or
//!   killed gate never waves a branch through.
//! - **event-reaction** ([`Dispatcher::dispatch_event`]) — runs off the durable
//!   `Transition`/SSE event stream, **FAIL-OPEN**: a faulting reaction is logged
//!   and the event proceeds. The stream is cycle-guarded so a reaction loop cannot
//!   infinite-spawn (the repo's documented `auto_pr` flaw is not to be repeated).
//!
//! The dispatcher is the *only* place the breaker's trip is acted on: when an
//! extension crosses its consecutive-failure threshold the dispatcher
//! **auto-disables it in the registry** and hands the [`BreakerAlert`] to its
//! alert sink (design §3.4: "auto-disable + surfaced alert"). The breaker and
//! cycle guard themselves stay pure, registry-free leaves.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::breaker::{BreakerAlert, CircuitBreaker};
use crate::cycle::{CycleConfig, CycleGuard, EventOrigin};
use crate::engine::ExtEngine;
use crate::fault::ExtensionFault;
use crate::gate::{GateCheckHost, GateInput, Verdict, VerdictKind};
use crate::reaction::{ActionKind, ExtEvent, ReactionHost};
use crate::registry::Registry;

/// The exported-interface name a gate-check extension is indexed under (matches
/// the WIT `interface gate-check` and the manifest's `extension-points`).
pub const GATE_CHECK_INTERFACE: &str = "gate-check";

/// The exported-interface name an event-reaction extension is indexed under.
pub const EVENT_REACTION_INTERFACE: &str = "event-reaction";

/// A hard ceiling on how many events one [`dispatch_event`](Dispatcher::dispatch_event)
/// call will process, including extension-emitted follow-ups. The cycle guard's
/// depth + rate limits already bound a chain; this is a belt-and-braces cap so a
/// single dispatch can never run unboundedly even under a misconfigured guard.
const MAX_EVENTS_PER_DISPATCH: usize = 256;

/// Apply the gate-check fail-closed policy to one invocation result.
///
/// A successful guest verdict is taken as-is. Any host-boundary fault — trap,
/// panic, fuel/epoch kill, timeout, OOM — is mapped to a **`fail`** verdict (never
/// `skip`), so a misbehaving or absent gate blocks the land rather than waving it
/// through (design §3.4). The fault is preserved in the message for operator
/// surfacing.
pub fn gate_verdict_fail_closed(result: Result<Verdict, ExtensionFault>) -> Verdict {
    match result {
        Ok(verdict) => verdict,
        Err(fault) => Verdict {
            kind: VerdictKind::Fail,
            message: format!("gate check failed closed: {fault}"),
        },
    }
}

/// Loads an extension's `.wasm` component bytes by content address (sha256).
///
/// A seam so the dispatcher stays decoupled from where bytes live — the daemon
/// backs this with the content-addressed `BlobStore` (design §3.5); tests back it
/// with an in-memory map. Returns a boxed future to keep the trait object-safe
/// without pulling in `async_trait`.
pub trait ComponentLoader: Send + Sync {
    /// Fetch the component bytes stored under `sha256`.
    fn load<'a>(
        &'a self,
        sha256: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>>;
}

/// Receives a [`BreakerAlert`] when an extension auto-disables (design §3.4:
/// "surfaced alert"). The default [`LogAlertSink`] traces it at `error`; the
/// daemon can swap in a sink that also writes a durable operator notification.
pub trait AlertSink: Send + Sync {
    /// Surface a breaker trip.
    fn alert(&self, alert: BreakerAlert);
}

/// The default alert sink: trace the trip at `error` level.
#[derive(Debug, Default, Clone, Copy)]
pub struct LogAlertSink;

impl AlertSink for LogAlertSink {
    fn alert(&self, alert: BreakerAlert) {
        tracing::error!(
            ext = %alert.ext_id,
            consecutive = alert.consecutive,
            threshold = alert.threshold,
            "{alert}"
        );
    }
}

/// Tuning for a [`Dispatcher`].
#[derive(Debug, Clone, Copy)]
pub struct DispatcherConfig {
    /// Consecutive failures before an extension's breaker trips.
    pub breaker_threshold: u32,
    /// The event cycle guard's bounds.
    pub cycle: CycleConfig,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        Self {
            breaker_threshold: crate::breaker::DEFAULT_THRESHOLD,
            cycle: CycleConfig::default(),
        }
    }
}

/// A host-side lifecycle event fed into the event-reaction path. The engine builds
/// one from a durable `Transition` (a `LiveEvent::Transition` off the store's
/// event bus); the dispatcher carries the cycle guard's [`EventOrigin`] on it so an
/// extension-emitted follow-up descends correctly.
#[derive(Debug, Clone)]
pub struct HostEvent {
    /// Event kind (`transition` for a host lifecycle change, `ext:<id>:<name>` for
    /// an extension-emitted one).
    pub kind: String,
    /// The task the event concerns (empty if none).
    pub task_id: String,
    /// The run/workflow the task belongs to (empty if standalone).
    pub run_id: String,
    /// Transition edge: from-status.
    pub from_status: String,
    /// Transition edge: to-status.
    pub to_status: String,
    /// The cycle guard's provenance (origin tag + depth).
    pub origin: EventOrigin,
}

impl HostEvent {
    /// A host-origin transition event — the root of any causal chain.
    #[must_use]
    pub fn transition(
        task_id: impl Into<String>,
        run_id: impl Into<String>,
        from: impl Into<String>,
        to: impl Into<String>,
    ) -> Self {
        Self {
            kind: "transition".to_owned(),
            task_id: task_id.into(),
            run_id: run_id.into(),
            from_status: from.into(),
            to_status: to.into(),
            origin: EventOrigin::host(),
        }
    }

    /// Project to the WIT guest record.
    fn to_ext_event(&self) -> ExtEvent {
        ExtEvent {
            kind: self.kind.clone(),
            task_id: self.task_id.clone(),
            run_id: self.run_id.clone(),
            from_status: self.from_status.clone(),
            to_status: self.to_status.clone(),
            origin: self.origin.origin.clone().unwrap_or_default(),
            depth: self.origin.depth,
        }
    }
}

/// The aggregate gate-check verdict over all enabled gate-check extensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// Every gate-check extension passed or abstained (or there were none).
    Pass,
    /// At least one gate-check extension blocked the land; the reason aggregates
    /// the failing extensions' messages.
    Block(String),
}

impl GateDecision {
    /// Whether the land may proceed.
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(self, GateDecision::Pass)
    }
}

/// Resolves and invokes extensions at each extension point, owning the per-point
/// policy, the circuit breaker, and the event cycle guard.
///
/// Cheap to share behind an `Arc`: the registry is already an `Arc<RwLock<_>>`, the
/// engine is `Clone` (an `Arc` inside), and the breaker/cycle guard are internally
/// `Mutex`-guarded.
pub struct Dispatcher {
    engine: ExtEngine,
    registry: Arc<RwLock<Registry>>,
    loader: Arc<dyn ComponentLoader>,
    breaker: CircuitBreaker,
    cycle: CycleGuard,
    alerts: Arc<dyn AlertSink>,
}

impl Dispatcher {
    /// Build a dispatcher over a shared registry + engine, loading component bytes
    /// through `loader`. Uses the default [`LogAlertSink`].
    #[must_use]
    pub fn new(
        engine: ExtEngine,
        registry: Arc<RwLock<Registry>>,
        loader: Arc<dyn ComponentLoader>,
        cfg: DispatcherConfig,
    ) -> Self {
        Self {
            engine,
            registry,
            loader,
            breaker: CircuitBreaker::new(cfg.breaker_threshold),
            cycle: CycleGuard::new(cfg.cycle),
            alerts: Arc::new(LogAlertSink),
        }
    }

    /// Swap in a custom alert sink (builder style).
    #[must_use]
    pub fn with_alert_sink(mut self, sink: Arc<dyn AlertSink>) -> Self {
        self.alerts = sink;
        self
    }

    /// The circuit breaker (for an operator re-enable to reset it, or for tests).
    #[must_use]
    pub fn breaker(&self) -> &CircuitBreaker {
        &self.breaker
    }

    /// The event cycle guard.
    #[must_use]
    pub fn cycle(&self) -> &CycleGuard {
        &self.cycle
    }

    /// Snapshot the (id, wasm_sha256) of every enabled extension exporting
    /// `interface` whose breaker has **not** tripped, dropping the registry read
    /// lock before any `.await`. Returns install order (the dispatch order).
    fn dispatch_set(&self, interface: &str) -> Vec<(String, String)> {
        let reg = self.registry.read().expect("registry lock poisoned");
        reg.find_by_export(interface)
            .into_iter()
            .filter(|r| !self.breaker.is_tripped(&r.id))
            .map(|r| (r.id.clone(), r.wasm_sha256.clone()))
            .collect()
    }

    /// Auto-disable a tripped extension in the registry and surface the alert
    /// (design §3.4). Keeps the in-memory dispatch index in lock-step; the store
    /// row is updated by the caller mirroring this on the next reconcile/boot.
    fn trip(&self, alert: BreakerAlert) {
        let id = alert.ext_id.clone();
        {
            let mut reg = self.registry.write().expect("registry lock poisoned");
            reg.set_enabled(&id, false);
        }
        tracing::warn!(ext = %id, "circuit breaker tripped — extension auto-disabled");
        self.alerts.alert(alert);
    }

    /// Run **all enabled gate-check extensions** for `input`, FAIL-CLOSED.
    ///
    /// Each extension is invoked in install order under the full resource regime.
    /// A guest verdict is taken as-is; a host-boundary fault is mapped to `fail`
    /// ([`gate_verdict_fail_closed`]) and fed to the breaker — *only faults* count
    /// toward a gate-check breaker, never a clean `fail` verdict, since a gate that
    /// correctly blocks bad branches is doing its job and must not auto-disable
    /// itself (the breaker exists for a *faulting* guest here; the rejection-counts
    /// clause of §3.4 is for the fail-closed *mutator* point). A clean run (pass or
    /// fail) resets the breaker's fault streak.
    ///
    /// The aggregate blocks the land if **any** extension blocks. With no enabled
    /// gate-check extensions this returns [`GateDecision::Pass`] (the command gate
    /// is unaffected — this layers on top of it).
    pub async fn run_gate_checks(&self, input: GateInput) -> GateDecision {
        let set = self.dispatch_set(GATE_CHECK_INTERFACE);
        let mut blocks: Vec<String> = Vec::new();

        for (id, sha) in set {
            let result = self.evaluate_gate(&sha, &input).await;
            // Whether the *execution* faulted (vs returned a clean verdict).
            let faulted = result.is_err();
            let verdict = gate_verdict_fail_closed(result);

            if faulted {
                if let Some(alert) = self.breaker.record_fault(&id, &verdict.message) {
                    self.trip(alert);
                }
            } else {
                // A clean execution (pass or fail) clears the fault streak.
                self.breaker.record_success(&id);
            }

            if verdict.kind == VerdictKind::Fail {
                tracing::info!(ext = %id, "gate-check extension blocked the land: {}", verdict.message);
                blocks.push(format!("[{id}] {}", verdict.message));
            }
        }

        if blocks.is_empty() {
            GateDecision::Pass
        } else {
            GateDecision::Block(blocks.join("; "))
        }
    }

    /// Load + compile + invoke one gate-check guest. All failure modes surface as a
    /// typed [`ExtensionFault`]; this never panics.
    async fn evaluate_gate(
        &self,
        sha256: &str,
        input: &GateInput,
    ) -> Result<Verdict, ExtensionFault> {
        let bytes = self
            .loader
            .load(sha256)
            .await
            .map_err(ExtensionFault::Load)?;
        let host = GateCheckHost::from_bytes(self.engine.clone(), &bytes)?;
        let outcome = host.evaluate(input.clone()).await?;
        Ok(outcome.verdict)
    }

    /// Dispatch one host event to **all enabled event-reaction extensions**,
    /// FAIL-OPEN and CYCLE-GUARDED.
    ///
    /// Every reaction runs under the resource regime; a fault is logged and fed to
    /// the breaker but **never** blocks anything (fail-open). Extension-emitted
    /// follow-up events re-enter the stream only through the cycle guard: dropped
    /// if the causal chain is too deep or re-enters the emitting extension, and
    /// capped by the per-extension per-window emit rate limit (design §3.4). This
    /// is what makes a reaction loop impossible.
    ///
    /// Returns the number of reactions actually invoked (handy for tests/metrics).
    pub async fn dispatch_event(&self, event: HostEvent) -> usize {
        let set = self.dispatch_set(EVENT_REACTION_INTERFACE);
        if set.is_empty() {
            return 0;
        }

        let mut queue: std::collections::VecDeque<HostEvent> = std::collections::VecDeque::new();
        queue.push_back(event);
        let mut invoked = 0usize;
        let mut processed = 0usize;

        while let Some(evt) = queue.pop_front() {
            processed += 1;
            if processed > MAX_EVENTS_PER_DISPATCH {
                tracing::warn!(
                    "event dispatch hit the per-call ceiling ({MAX_EVENTS_PER_DISPATCH}); \
                     dropping the rest of the causal chain (cycle-guard backstop)"
                );
                break;
            }

            for (id, sha) in &set {
                // CYCLE GUARD: drop a too-deep or self-reentrant delivery.
                let admission = self.cycle.admit(id, &evt.origin);
                if !admission.is_admitted() {
                    tracing::debug!(ext = %id, ?admission, "cycle guard dropped event delivery");
                    continue;
                }

                invoked += 1;
                match self.react(sha, &evt).await {
                    Ok(actions) => {
                        self.breaker.record_success(id);
                        self.enqueue_emitted(id, &evt, actions, &mut queue);
                    }
                    Err(fault) => {
                        // FAIL-OPEN: log + feed the breaker, never block.
                        tracing::warn!(ext = %id, "event-reaction extension faulted (fail-open): {fault}");
                        if let Some(alert) = self.breaker.record_fault(id, &fault.to_string()) {
                            self.trip(alert);
                        }
                    }
                }
            }
        }
        invoked
    }

    /// Turn a reaction's `emit-event` actions into child events, each subject to the
    /// per-extension emit rate limit and stamped with a descended origin (origin =
    /// emitting ext, depth = parent + 1).
    fn enqueue_emitted(
        &self,
        ext_id: &str,
        parent: &HostEvent,
        actions: Vec<crate::reaction::ExtAction>,
        queue: &mut std::collections::VecDeque<HostEvent>,
    ) {
        for action in actions {
            match action.kind {
                ActionKind::EmitEvent => {
                    // RATE LIMIT: a guest that emits distinct events fast is capped.
                    if !self.cycle.allow_emit(ext_id) {
                        tracing::warn!(
                            ext = %ext_id,
                            "emit rate limit hit — dropping emitted event (cycle-guard backstop)"
                        );
                        continue;
                    }
                    let kind = if action.event_kind.is_empty() {
                        format!("ext:{ext_id}")
                    } else {
                        format!("ext:{ext_id}:{}", action.event_kind)
                    };
                    queue.push_back(HostEvent {
                        kind,
                        task_id: parent.task_id.clone(),
                        run_id: parent.run_id.clone(),
                        from_status: String::new(),
                        to_status: String::new(),
                        origin: parent.origin.descend(ext_id),
                    });
                }
                ActionKind::Notify => {
                    tracing::info!(ext = %ext_id, "extension notify: {}", action.message);
                }
                ActionKind::None => {}
            }
        }
    }

    /// Load + compile + invoke one event-reaction guest.
    async fn react(
        &self,
        sha256: &str,
        event: &HostEvent,
    ) -> Result<Vec<crate::reaction::ExtAction>, ExtensionFault> {
        let bytes = self
            .loader
            .load(sha256)
            .await
            .map_err(ExtensionFault::Load)?;
        let host = ReactionHost::from_bytes(self.engine.clone(), &bytes)?;
        host.react(event.to_ext_event()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineLimits;
    use crate::gate::DiffStat;
    use crate::registry::InstallRequest;
    use lazybones_store::ExtensionSource;

    /// Path to the checked-in example gate-check guest component — the *runnable*
    /// component the loader serves. (It has no embedded manifest; the registry is
    /// fed a separate manifest-bearing stub, mirroring how install reads the
    /// manifest while dispatch loads the bytes.)
    fn fixture_wasm() -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/wasm/gate-check-example.wasm");
        std::fs::read(p).expect("read fixture wasm")
    }

    /// A manifest declaring a single `gate-check` extension point.
    const GATE_MANIFEST: &str = "\
name = \"gate-guard\"
version = \"0.1.0\"
wit-world = \"extension\"
extension-points = [\"gate-check\"]
capabilities = [\"log\"]
";

    /// LEB128-encode an unsigned int (wasm custom-section sizes are LEB128).
    fn uleb(mut n: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (n & 0x7f) as u8;
            n >>= 7;
            if n != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if n == 0 {
                break;
            }
        }
        out
    }

    /// A minimal wasm binary carrying `toml` in the manifest custom section — the
    /// registry only needs to parse the embedded manifest to index the export.
    fn wasm_with_manifest(toml: &str) -> Vec<u8> {
        let mut wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let name = crate::manifest::MANIFEST_SECTION.as_bytes();
        let mut body = Vec::new();
        body.extend_from_slice(&uleb(name.len() as u64));
        body.extend_from_slice(name);
        body.extend_from_slice(toml.as_bytes());
        wasm.push(0x00); // custom section id
        wasm.extend_from_slice(&uleb(body.len() as u64));
        wasm.extend_from_slice(&body);
        wasm
    }

    fn gate_input(summary: &str) -> GateInput {
        GateInput {
            task_id: "t".into(),
            task_summary: summary.into(),
            diff: DiffStat {
                files_changed: 3,
                insertions: 10,
                deletions: 2,
            },
        }
    }

    /// A loader serving fixed bytes for every sha (the test only installs one ext).
    struct FixedLoader(Vec<u8>);
    impl ComponentLoader for FixedLoader {
        fn load<'a>(
            &'a self,
            _sha256: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>> {
            let bytes = self.0.clone();
            Box::pin(async move { Ok(bytes) })
        }
    }

    /// A loader that always fails — every invocation becomes a Load fault.
    struct FailingLoader;
    impl ComponentLoader for FailingLoader {
        fn load<'a>(
            &'a self,
            _sha256: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>> {
            Box::pin(async move { Err("blob gone".to_owned()) })
        }
    }

    /// Records alerts so a test can assert the trip surfaced.
    #[derive(Default)]
    struct CountingSink(std::sync::Mutex<Vec<BreakerAlert>>);
    impl AlertSink for CountingSink {
        fn alert(&self, alert: BreakerAlert) {
            self.0.lock().unwrap().push(alert);
        }
    }

    /// Install a gate-check extension into the registry from a manifest-bearing
    /// stub (so it indexes under `gate-check`). The runnable bytes are served
    /// separately by the test's loader.
    fn install_gate_fixture(reg: &mut Registry, id: &str) {
        let stub = wasm_with_manifest(GATE_MANIFEST);
        reg.install(InstallRequest {
            id: id.to_owned(),
            component: &stub,
            granted_caps: vec![],
            source: ExtensionSource::Upload,
            expected_sha256: None,
            enabled: true,
            created_at: "2026-06-24T00:00:00Z".to_owned(),
            record: None,
        })
        .expect("install fixture");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_extensions_passes_the_gate() {
        let engine = ExtEngine::new(EngineLimits::default()).unwrap();
        let reg = Arc::new(RwLock::new(Registry::new()));
        let d = Dispatcher::new(
            engine,
            reg,
            Arc::new(FixedLoader(vec![])),
            DispatcherConfig::default(),
        );
        assert_eq!(d.run_gate_checks(gate_input("x")).await, GateDecision::Pass);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gate_extension_verdict_is_aggregated_fail_closed() {
        let bytes = fixture_wasm();
        let engine = ExtEngine::new(EngineLimits::default()).unwrap();
        let mut reg = Registry::new();
        install_gate_fixture(&mut reg, "gate-1");
        let reg = Arc::new(RwLock::new(reg));
        let d = Dispatcher::new(
            engine,
            reg,
            Arc::new(FixedLoader(bytes)),
            DispatcherConfig::default(),
        );
        // The example guest passes a normal change and fails when the summary
        // contains "fail". A pass aggregates to Pass...
        assert!(d.run_gate_checks(gate_input("normal change")).await.is_pass());
        // ...and a failing verdict aggregates to Block, carrying the ext id.
        match d.run_gate_checks(gate_input("please fail this")).await {
            GateDecision::Block(reason) => assert!(reason.contains("gate-1")),
            GateDecision::Pass => panic!("expected a block on the fail summary"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn breaker_trips_and_auto_disables_a_faulting_gate() {
        let engine = ExtEngine::new(EngineLimits::default()).unwrap();
        let mut reg = Registry::new();
        install_gate_fixture(&mut reg, "bad");
        let reg = Arc::new(RwLock::new(reg));
        let sink = Arc::new(CountingSink::default());
        // A failing loader makes every gate eval a Load fault.
        let d = Dispatcher::new(
            engine,
            reg.clone(),
            Arc::new(FailingLoader),
            DispatcherConfig {
                breaker_threshold: 3,
                ..DispatcherConfig::default()
            },
        )
        .with_alert_sink(sink.clone());

        // Each run faults (fail-closed → Block) and increments the breaker. The
        // third trips it and auto-disables the extension in the registry.
        for _ in 0..3 {
            assert!(matches!(
                d.run_gate_checks(gate_input("x")).await,
                GateDecision::Block(_)
            ));
        }
        assert!(d.breaker().is_tripped("bad"));
        assert_eq!(sink.0.lock().unwrap().len(), 1, "alert surfaced once");
        assert!(
            !reg.read().unwrap().get("bad").unwrap().enabled,
            "auto-disabled in the registry"
        );
        // Once disabled it drops out of the dispatch set → the gate passes (no
        // enabled gate-check extensions left), and no further alerts fire.
        assert!(d.run_gate_checks(gate_input("x")).await.is_pass());
        assert_eq!(sink.0.lock().unwrap().len(), 1, "no repeat alert");
    }
}
