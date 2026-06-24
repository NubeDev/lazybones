//! Integration test for the `weather` extension point (the `http-fetch` example).
//!
//! Loads the example `weather` guest component and runs it through [`WeatherHost`],
//! proving the WASM guest does the outbound fetch itself under the allowlist.
//!
//! Marked `#[ignore]` because it makes a real network call to Open-Meteo; run it
//! explicitly with `cargo test -p lazybones-ext --test weather -- --ignored`.

use std::path::PathBuf;

use lazybones_ext::{ExtEngine, EngineLimits, HostAllowlist, WeatherHost, WeatherQuery};

fn guest_wasm() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/weather/dist/weather.wasm")
}

fn allowlist() -> HostAllowlist {
    HostAllowlist::from_hosts(["geocoding-api.open-meteo.com", "api.open-meteo.com"])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "makes a real network call to Open-Meteo"]
async fn guest_fetches_real_weather() {
    let engine = ExtEngine::new(EngineLimits::default()).expect("engine");
    let bytes = std::fs::read(guest_wasm()).expect("read guest wasm");
    let host = WeatherHost::from_bytes(engine, &bytes, allowlist()).expect("load guest");

    let outcome = host
        .fetch(WeatherQuery {
            location: "Berlin".to_string(),
        })
        .await
        .expect("host fault");

    let r = &outcome.result;
    assert!(r.error.is_none(), "guest error: {:?}", r.error);
    assert!(r.location.contains("Berlin"), "location: {}", r.location);
    assert!((-90.0..=90.0).contains(&r.latitude));
    assert!(r.temperature_c > -90.0 && r.temperature_c < 70.0, "temp: {}", r.temperature_c);
    eprintln!(
        "Berlin: {:.1}°C, wind {:.1} km/h, {} (code {}) @ {}",
        r.temperature_c, r.wind_kph, r.description, r.weather_code, r.observed_at
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "makes a real network call to Open-Meteo"]
async fn denied_host_is_blocked_by_allowlist() {
    // An empty allowlist must deny the guest's outbound call. The host rejects the
    // request in the `wasi:http` hook, which traps the guest — surfaced as a host
    // fault (the API maps this to `faulted: true` + an `error`), NOT a clean
    // guest return. Either a fault or a guest-caught error is acceptable; what
    // must NOT happen is a successful weather result.
    let engine = ExtEngine::new(EngineLimits::default()).expect("engine");
    let bytes = std::fs::read(guest_wasm()).expect("read guest wasm");
    let host = WeatherHost::from_bytes(engine, &bytes, HostAllowlist::new()).expect("load guest");

    match host
        .fetch(WeatherQuery {
            location: "Berlin".to_string(),
        })
        .await
    {
        Ok(outcome) => assert!(
            outcome.result.error.is_some(),
            "deny-all allowlist should have blocked the fetch, got {:?}",
            outcome.result
        ),
        Err(fault) => {
            let msg = fault.to_string();
            assert!(
                msg.contains("allowlist") || msg.to_lowercase().contains("denied"),
                "expected an allowlist-denial fault, got: {msg}"
            );
        }
    }
}
