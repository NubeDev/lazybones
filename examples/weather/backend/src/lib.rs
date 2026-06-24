//! Backend WASM guest for the `weather` example extension.
//!
//! Implements the `lazybones:ext/weather` interface (the `weather-provider`
//! world, see the host's `crates/lazybones-ext/wit`). **The backend does 100% of
//! the work:** given a free-text place name, this guest
//!
//!   1. geocodes it via Open-Meteo's keyless geocoding API, then
//!   2. fetches current conditions via Open-Meteo's keyless forecast API,
//!
//! both over `wasi:http` (the `http-fetch` capability). It parses the JSON and
//! returns a typed `weather-result`. The frontend never touches the weather API —
//! it only renders what this returns.
//!
//! Open-Meteo needs no API key. The host bounds our outbound reach to the two
//! Open-Meteo hosts via the install-time allowlist (design §3.3), so even though
//! the guest *could* construct any request, only those two resolve.

// Bindings for OUR world: export the `weather` interface. We do not ask
// wit-bindgen to generate the `wasi:http` import bindings — `waki` owns those
// (its own `wit_bindgen::generate!` brings the `wasi:http/proxy` imports the
// `Client` uses), and the component's single set of `wasi:http` imports is shared
// by both. We therefore generate ONLY the export side here.
wit_bindgen::generate!({
    path: "../../../crates/lazybones-ext/wit",
    world: "weather-provider",
    // The `wasi:http`/`wasi:io`/`wasi:clocks` imports this world declares are
    // satisfied by waki's generated bindings (same `wasi:*@0.2` packages), so we
    // don't re-emit them here and risk duplicate definitions.
    generate_all,
});

use exports::lazybones::ext::weather::{Guest, WeatherQuery, WeatherResult};
use serde::Deserialize;

struct Component;

/// An all-zero result carrying just an error message (the guest ran but could not
/// resolve the query). This is a clean return, not a host fault.
fn err_result(location: &str, message: impl Into<String>) -> WeatherResult {
    WeatherResult {
        location: location.to_string(),
        latitude: 0.0,
        longitude: 0.0,
        temperature_c: 0.0,
        wind_kph: 0.0,
        weather_code: 0,
        description: String::new(),
        observed_at: String::new(),
        error: Some(message.into()),
    }
}

#[derive(Deserialize)]
struct GeoResponse {
    #[serde(default)]
    results: Vec<GeoHit>,
}

#[derive(Deserialize)]
struct GeoHit {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    country: Option<String>,
}

#[derive(Deserialize)]
struct ForecastResponse {
    current: CurrentBlock,
}

#[derive(Deserialize)]
struct CurrentBlock {
    time: String,
    temperature_2m: f64,
    wind_speed_10m: f64,
    weather_code: u32,
}

/// Fetch a URL and deserialize its JSON body, mapping every failure to a string.
fn get_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    let resp = waki::Client::new()
        .get(url)
        .send()
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status_code();
    let body = resp.body().map_err(|e| format!("read body failed: {e}"))?;
    if status != 200 {
        return Err(format!("upstream returned HTTP {status}"));
    }
    serde_json::from_slice(&body).map_err(|e| format!("parse failed: {e}"))
}

impl Guest for Component {
    fn get_weather(query: WeatherQuery) -> WeatherResult {
        let location = query.location.trim();
        if location.is_empty() {
            return err_result("", "please enter a place name");
        }

        // 1. Geocode the place name → lat/lon (keyless Open-Meteo geocoding).
        let geo_url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=en&format=json",
            url_encode(location)
        );
        let geo: GeoResponse = match get_json(&geo_url) {
            Ok(g) => g,
            Err(e) => return err_result(location, e),
        };
        let Some(hit) = geo.results.into_iter().next() else {
            return err_result(location, format!("no place found matching \"{location}\""));
        };

        let canonical = match &hit.country {
            Some(c) if !c.is_empty() => format!("{}, {}", hit.name, c),
            _ => hit.name.clone(),
        };

        // 2. Fetch current conditions for that coordinate (keyless forecast).
        let fc_url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,wind_speed_10m,weather_code",
            hit.latitude, hit.longitude
        );
        let fc: ForecastResponse = match get_json(&fc_url) {
            Ok(f) => f,
            Err(e) => return err_result(&canonical, e),
        };

        WeatherResult {
            location: canonical,
            latitude: hit.latitude,
            longitude: hit.longitude,
            temperature_c: fc.current.temperature_2m,
            wind_kph: fc.current.wind_speed_10m,
            weather_code: fc.current.weather_code,
            description: wmo_description(fc.current.weather_code).to_string(),
            observed_at: fc.current.time,
            error: None,
        }
    }
}

/// Minimal percent-encoding for the place-name query parameter (alnum + a few
/// safe chars pass through; everything else is `%XX`). Avoids a url crate dep.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Map a WMO weather interpretation code to a human-readable description.
/// (https://open-meteo.com/en/docs — the `weather_code` table.)
fn wmo_description(code: u32) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 => "Fog",
        48 => "Depositing rime fog",
        51 => "Light drizzle",
        53 => "Moderate drizzle",
        55 => "Dense drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Slight rain",
        63 => "Moderate rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Slight snow fall",
        73 => "Moderate snow fall",
        75 => "Heavy snow fall",
        77 => "Snow grains",
        80 => "Slight rain showers",
        81 => "Moderate rain showers",
        82 => "Violent rain showers",
        85 => "Slight snow showers",
        86 => "Heavy snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Unknown",
    }
}

export!(Component);
