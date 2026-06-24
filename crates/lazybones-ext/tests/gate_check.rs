//! Host tests for the gate-check extension point (design §5 P0 acceptance):
//!
//! 1. Load the example guest component and assert it returns the expected
//!    verdicts (pass / fail / skip) through the real instantiate + `call_async`
//!    path, and log the measured **cold instantiation latency**.
//! 2. Assert a **runaway guest is killed** — once by the CPU **fuel** budget and
//!    once by the **epoch** (wall-clock) deadline — and surfaces as a typed
//!    [`ExtensionFault`], never a panic.
//! 3. Confirm the dispatcher's **fail-closed** policy maps that fault to `fail`.

use std::time::Duration;

use lazybones_ext::dispatch::gate_verdict_fail_closed;
use lazybones_ext::engine::EngineLimits;
use lazybones_ext::{DiffStat, ExtEngine, ExtensionFault, GateCheckHost, GateInput, VerdictKind};

/// Path to the checked-in example guest component (built from
/// `fixtures/examples/gate-check-example` for `wasm32-wasip2`).
fn fixture_wasm() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/wasm/gate-check-example.wasm")
}

fn input(summary: &str, files: u32, ins: u32, del: u32) -> GateInput {
    GateInput {
        task_id: "task-123".to_string(),
        task_summary: summary.to_string(),
        diff: DiffStat {
            files_changed: files,
            insertions: ins,
            deletions: del,
        },
    }
}

fn init_tracing() {
    // Best-effort; ignore "already set" when tests share a process.
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter("info")
        .try_init();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loads_guest_and_returns_expected_verdicts() {
    init_tracing();
    let engine = ExtEngine::new(EngineLimits::default()).expect("engine");
    let host = GateCheckHost::from_file(engine, fixture_wasm()).expect("load guest");

    // Pass: a normal change with files touched.
    let out = host
        .evaluate(input("add feature", 3, 40, 5))
        .await
        .expect("invoke");
    assert_eq!(out.verdict.kind, VerdictKind::Pass, "{:?}", out.verdict);
    assert!(out.verdict.message.contains("3 file"));

    // Log the measured cold instantiation latency (design §3.4: a measured P0 input).
    tracing::info!(
        cold_instantiation_us = out.instantiation.as_micros() as u64,
        "measured cold instantiation latency"
    );
    println!(
        "measured cold instantiation latency: {} µs",
        out.instantiation.as_micros()
    );

    // Fail: summary asks for failure.
    let out = host
        .evaluate(input("please fail this", 2, 1, 1))
        .await
        .expect("invoke");
    assert_eq!(out.verdict.kind, VerdictKind::Fail, "{:?}", out.verdict);

    // Skip: empty diff.
    let out = host
        .evaluate(input("noop", 0, 0, 0))
        .await
        .expect("invoke");
    assert_eq!(out.verdict.kind, VerdictKind::Skip, "{:?}", out.verdict);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runaway_guest_is_killed_by_fuel() {
    init_tracing();
    // Default limits: fuel runs out long before the 500 ms epoch budget, so a
    // tight loop is killed by fuel exhaustion.
    let engine = ExtEngine::new(EngineLimits::default()).expect("engine");
    let host = GateCheckHost::from_file(engine, fixture_wasm()).expect("load guest");

    let result = host.evaluate(input("runaway", 1, 0, 0)).await;
    let fault = result.expect_err("runaway guest must not return a verdict");
    tracing::info!(?fault, "runaway guest killed");
    assert!(
        matches!(fault, ExtensionFault::FuelExhausted),
        "expected fuel kill, got {fault:?}"
    );
    assert!(fault.is_resource_kill());

    // Fail-closed policy turns the kill into a `fail` verdict.
    let verdict = gate_verdict_fail_closed(Err(fault));
    assert_eq!(verdict.kind, VerdictKind::Fail);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runaway_guest_is_killed_by_epoch() {
    init_tracing();
    // Effectively unlimited fuel, tiny wall-clock budget: this forces the kill to
    // come from epoch interruption (the background ticker) rather than fuel,
    // exercising that limiter independently.
    let limits = EngineLimits {
        fuel: u64::MAX,
        wall_clock: Duration::from_millis(50),
        max_memory_bytes: 64 * 1024 * 1024,
        epoch_tick: Duration::from_millis(2),
    };
    let engine = ExtEngine::new(limits).expect("engine");
    let host = GateCheckHost::from_file(engine, fixture_wasm()).expect("load guest");

    let result = host.evaluate(input("runaway", 1, 0, 0)).await;
    let fault = result.expect_err("runaway guest must not return a verdict");
    tracing::info!(?fault, "runaway guest killed by epoch");
    assert!(
        matches!(fault, ExtensionFault::Deadline),
        "expected epoch deadline kill, got {fault:?}"
    );
    assert!(fault.is_resource_kill());
}
