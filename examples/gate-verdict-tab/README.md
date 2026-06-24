# `gate-verdict-tab` — end-to-end example extension

A single example extension that exercises **both planes** of the lazybones
extension system (`docs/design/extension-system.md`):

- **Backend (WASM):** a `gate-check` guest component — the same tiny guest as the
  `crates/lazybones-ext/fixtures` gate-check fixture — repackaged as an
  **installable extension** with an embedded `lazybones.ext.toml` manifest.
- **Frontend (Module Federation):** a federated remote that exposes a
  `task-detail.tab` rendering the extension's **latest gate verdict for the
  currently-open task**, via `@lazybones/ext-sdk` + the daemon REST API.

```
examples/gate-verdict-tab/
├── lazybones.ext.toml      # the manifest (embedded into the .wasm on build)
├── dist/
│   └── gate-verdict-tab.wasm   # prebuilt installable component (manifest embedded)
├── backend/                # the WASM gate-check guest
│   ├── src/lib.rs          # pass / fail / skip verdict policy
│   ├── build.sh            # build + embed manifest -> dist/*.wasm
│   └── embed-manifest.py   # appends the lazybones.ext.toml custom section
└── frontend/               # the federated UI remote
    ├── vite.config.ts      # MF remote: exposes ./mount, shares the host singletons
    └── src/
        ├── mount.tsx           # registers the task-detail.tab slot
        └── GateVerdictPanel.tsx# invokes the gate over REST and renders the verdict
```

## The manifest (`lazybones.ext.toml`)

The manifest is the **source of truth** for the extension's declared identity. It
declares the required fields and is embedded into the component as a custom
section named `lazybones.ext.toml`, which the daemon reads on install:

```toml
name = "gate-verdict-tab"
version = "0.1.0"
wit-world = "extension"
extension-points = ["gate-check"]   # exported WIT interfaces (dispatch keys)
capabilities = ["log"]              # requested import surface (log is always granted)

[frontend]
entry = "remoteEntry.js"            # frontend-entry: the federated remote
exposed-module = "./mount"
sdk-range = "^0.1.0"
slots = ["task-detail.tab"]
```

## Build

A prebuilt, manifest-embedded `dist/gate-verdict-tab.wasm` is checked in, so you
can install without a toolchain. To rebuild it from source:

```sh
# requires: rustup target add wasm32-wasip2   (+ python3 for the embed step)
cd backend && ./build.sh
```

`build.sh` compiles the guest to a `wasm32-wasip2` **component**, then
`embed-manifest.py` appends the `lazybones.ext.toml` custom section the daemon
scans for — producing `dist/gate-verdict-tab.wasm`.

Build the frontend remote:

```sh
cd frontend
npm install
npm run build          # emits dist/remoteEntry.js + chunks
```

## End-to-end flow

1. **Install the backend** (requires the `extension` capability):

   ```sh
   curl -X POST "$LAZYBONES_URL/extensions?id=gate-verdict-tab" \
     -H "Authorization: Bearer $LOOP_TOKEN" \
     --data-binary @dist/gate-verdict-tab.wasm
   ```

   The daemon parses + validates the embedded manifest and stores the record
   **disabled with no grants** (default-deny). The response carries the parsed
   `frontend` descriptor mirrored from the manifest.

2. **Review + enable**, granting the requested capability:

   ```sh
   curl -X POST "$LAZYBONES_URL/extensions/gate-verdict-tab/grants" \
     -H "Authorization: Bearer $LOOP_TOKEN" \
     -d '{"capabilities":["log"]}'
   curl -X POST "$LAZYBONES_URL/extensions/gate-verdict-tab/enable" \
     -H "Authorization: Bearer $LOOP_TOKEN"
   ```

3. **Serve the frontend bundle.** Make the `frontend/dist/` output reachable
   under the daemon's frontend proxy so
   `GET /extensions/gate-verdict-tab/frontend/remoteEntry.js` resolves (design
   §4.3). The host fetches enabled remotes from `GET /extensions?frontend=1` on
   boot and registers each `remoteEntry.js` with the Module Federation runtime.

4. **Open any task in the UI.** The host imports the remote's `./mount` module
   and calls it once with an `ExtSdkHandle`; `mount.tsx` registers a **Gate** tab
   into the `task-detail.tab` slot. Selecting that tab renders
   `GateVerdictPanel`, which:

   - calls `GET /tasks/:id` for the task summary, then
   - calls `POST /extensions/gate-verdict-tab/invoke` with the `gate-check`
     export — the daemon compiles the guest, runs it under the
     fuel/epoch/memory/timeout sandbox, and returns the `pass` / `fail` / `skip`
     verdict (or a fail-closed verdict on a host fault), and
   - renders the verdict, refreshing whenever the task transitions (SSE
     `transition` events on the shared stream).

That is the full loop: **frontend tab → REST → host runtime → WASM gate guest →
verdict → rendered in the tab**, both planes of one installed extension.

> **Trust:** v1 frontend remotes are first-party / signed-only and run as
> fully-trusted in-origin JS. Everything the remote touches goes through
> `@lazybones/ext-sdk`; it never reaches around the SDK to `fetch` the daemon
> directly.
