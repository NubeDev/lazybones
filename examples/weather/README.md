# `weather` — end-to-end example extension (the **backend does 100%**)

A single example extension that exercises **both planes** of the lazybones
extension system (`docs/design/extension-system.md`) and proves the `http-fetch`
capability end-to-end — **the WASM backend fetches the weather itself**, the
frontend only renders it.

- **Backend (WASM):** a `weather` guest component (the `weather-provider` WIT
  world) that, holding the `http-fetch` grant, **imports `wasi:http` and dials a
  keyless weather API itself** — geocoding the place name, then fetching current
  conditions, then parsing the JSON. The host grants the (allowlisted) outbound
  authority and does nothing else.
- **Frontend (Module Federation):** a federated remote that registers a top-level
  **`route`** ("Weather" nav page) built with **shadcn UI**. It calls the
  backend's `weather` export over REST and renders the typed result. It **never**
  calls the weather API itself.

> Why this needed core work: before this example, the WIT contract only exposed
> `gate-check`/`event-reaction` and every guest ran with an **empty** WASI context
> — no guest could make an outbound call. Adding "the backend does 100%" meant
> wiring `wasmtime-wasi-http` into the host (`crates/lazybones-ext`), gated on the
> `http-fetch` grant and bounded by a `HostAllowlist`, plus a new `weather` WIT
> extension point. See `crates/lazybones-ext/src/{weather,caps}.rs`.

```
examples/weather/
├── lazybones.ext.toml      # the manifest (embedded into the .wasm on build)
├── dist/
│   └── weather.wasm        # prebuilt installable component (manifest embedded)
├── backend/                # the WASM weather guest (ui-src)
│   ├── src/lib.rs          # geocode + fetch + parse over wasi:http (waki client)
│   ├── build.sh            # build + embed manifest -> dist/weather.wasm
│   └── embed-manifest.py   # appends the lazybones.ext.toml custom section
└── frontend/               # the federated UI remote (ui)
    ├── vite.config.ts      # MF remote: exposes ./mount, shares host singletons
    └── src/
        ├── mount.tsx           # registers the `route` slot ("Weather" page)
        ├── WeatherPage.tsx     # invokes the weather export over REST, renders it
        └── components/ui.tsx   # bundled shadcn components (host design tokens)
```

## Why the WASM guest can reach the network (and only Open-Meteo)

Two host-side gates (design §3.3), both required:

1. **The `http-fetch` grant.** `HostState::with_http` only enables outbound
   sockets/DNS when the extension holds `http-fetch`. Without the grant the
   `wasi:http` interfaces still link (so the import resolves) but the network is
   off — the invoke route refuses the `weather` export entirely.
2. **The allowlist.** `crates/lazybones-api`'s invoke handler bounds this
   extension to `geocoding-api.open-meteo.com` + `api.open-meteo.com`. The
   `AllowlistHooks` in `crates/lazybones-ext/src/caps.rs` rejects any other host
   in the `wasi:http` send hook — the guest *could* construct any request, but
   only those two resolve. (Open-Meteo is keyless, so no secret is involved.)

## Build

A prebuilt, manifest-embedded `dist/weather.wasm` is checked in. To rebuild:

```sh
# requires: rustup target add wasm32-wasip2   (+ python3 for the embed step)
cd backend && ./build.sh
```

Build the frontend remote:

```sh
cd frontend
npm install            # resolves @lazybones/ext-sdk via a file: link
npm run build          # emits dist/remoteEntry.js + chunks
```

## End-to-end flow

1. **Install the backend** (requires the `extension` capability):

   ```sh
   curl -X POST "$LAZYBONES_URL/extensions?id=weather" \
     -H "Authorization: Bearer $LOOP_TOKEN" \
     --data-binary @dist/weather.wasm
   ```

   The daemon parses + validates the embedded manifest and stores the record
   **disabled with no grants** (default-deny).

2. **Grant `http-fetch` + enable.** `http-fetch` is the load-bearing grant — the
   guest cannot fetch without it:

   ```sh
   curl -X POST "$LAZYBONES_URL/extensions/weather/grants" \
     -H "Authorization: Bearer $LOOP_TOKEN" \
     -d '{"granted_caps":["log","http-fetch"]}'
   curl -X POST "$LAZYBONES_URL/extensions/weather/enable" \
     -H "Authorization: Bearer $LOOP_TOKEN"
   ```

3. **Test the backend directly** (proves the WASM guest does the fetch):

   ```sh
   curl -X POST "$LAZYBONES_URL/extensions/weather/invoke" \
     -H "Authorization: Bearer $LOOP_TOKEN" \
     -d '{"export":"weather","input":{"location":"Berlin"}}'
   # -> { "export":"weather", "weather":{ "location":"Berlin, Germany",
   #      "temperature_c":25.9, "wind_kph":3.5, "description":"Clear sky", ... },
   #      "faulted":false }
   ```

4. **Serve the frontend bundle.** There is no upload route yet, so the daemon's
   frontend proxy (`GET /extensions/weather/frontend/*`) reads files from the blob
   store under `{data_dir}/assets/ext-frontend/weather/`. Copy the built bundle
   there:

   ```sh
   cp -r frontend/dist/* "$LAZYBONES_DATA_DIR/assets/ext-frontend/weather/"
   ```

5. **Open the UI.** On boot the host fetches `GET /extensions?frontend=1`,
   registers the remote's `remoteEntry.js` with the Module Federation runtime, and
   imports `./mount` — which registers the **Weather** route into the sidebar.
   Open it, type a city, and the page calls the `weather` export; the daemon runs
   the WASM guest under the fuel/epoch/memory/timeout sandbox + the `http-fetch`
   allowlist, the guest fetches Open-Meteo itself, and the typed result renders.

That is the full loop: **frontend route → REST → host runtime → WASM guest →
`wasi:http` → Open-Meteo → parsed result → rendered**, with the backend doing
100% of the weather work.

> **Trust:** v1 frontend remotes are first-party / signed-only and run as
> fully-trusted in-origin JS. Everything the remote touches goes through
> `@lazybones/ext-sdk`; it never reaches around the SDK to `fetch` the daemon or
> the weather API directly.
