# lazybones

Dead-simple multi-agent build orchestration: run many AI coding sessions in
parallel and gate each one on a real green build. The orchestration *engine* is
[hcom](#the-loop); lazybones is the **durable queue + the gate** — a small Rust
daemon (`lazybonesd`) exposing a REST API over an embedded SurrealDB.

See [SCOPE.md](SCOPE.md) for the full design. This README is the quickstart.

## What's built

A standalone cargo workspace (will move to its own repo):

| Crate | Role |
| --- | --- |
| `lazybones-store` | Embedded SurrealDB boundary: task documents, the `depends_on` graph that drives readiness, the `event` run log, and the lifecycle state machine. |
| `lazybones-auth` | Scoped sessions + capability grants — the loop holds everything; an agent is scoped to its one task. |
| `lazybones-api` | The axum REST surface, one file per route, each mutation guarded by a scoped session. |
| `lazybones-engine` | The execution plane: the in-process Tokio scheduler (read ready → worktree → spawn via `hcom` → gate → advance) plus a typed `hcom` client. See [docs/scheduler.md](docs/scheduler.md). |
| `lazybones-ext` | The backend extension plane: a Wasmtime Component-Model host (fuel/epoch/memory limits, trap isolation), the capability sandbox, the registry, and lifecycle-point dispatch (gate-check, event reactions). See [docs/design/extension-system.md](docs/design/extension-system.md). |
| `lazybones-gh` | A thin async wrapper around the already-authenticated `gh` CLI — no token handling of our own. |
| `lazybones-render` | Pure markdown → Typst/PDF and markdown → HTML rendering for the document writer (no store dependency). |
| `lazybones-cli` | `lazybonesd` — parse config, open the store, serve; plus a one-shot workfile import. |

The React UI (`ui/`) is a **Module Federation host**: it ships `@lazybones/ext-sdk`
(`ui/packages/ext-sdk`) and a runtime loader so extension frontends mount into
named slots at runtime, no host rebuild. See [Extensions](#extensions) below.

## Quickstart

```sh
# from the lazybones/ directory
cargo build

# import the seed queue into the DB (idempotent; re-run to reconcile)
LAZYBONES_CONFIG=lazybones.yaml ./target/debug/lazybonesd import workfile.yaml

# serve the REST API
LAZYBONES_CONFIG=lazybones.yaml ./target/debug/lazybonesd serve
```

Then, over HTTP (the loop authenticates with the `LAZYBONES_LOOP_TOKEN`, default
`lazybones-loop`):

```sh
curl localhost:46787/health
curl localhost:46787/tasks                       # list all
curl localhost:46787/tasks?status=ready          # filter by status
curl -X POST localhost:46787/tasks/promote \
     -H 'authorization: Bearer lazybones-loop'  # promote pending→ready
```

## The lifecycle

```
  pending --deps done--> ready --claim--> running --DONE--> gating --green--> done
    any non-terminal state --unrecoverable--> blocked
    running --stale agent--> ready (reclaim)
```

`done` is earned: a task only reaches it from `gating` (the orchestrator re-runs
the gate after the agent's commit+push). The store's state machine rejects any
other path with `409`. Every transition is a queryable `event` row, not a log line.

## REST surface

| Method · path | Job | Cap |
| --- | --- | --- |
| `GET /health` | liveness | — |
| `GET /tasks` (`?status=`) | list / filter | — |
| `GET /tasks/:id` | one task | — |
| `GET /runs/:id` | full event history | — |
| `POST /workfile/sync` | idempotent seed import | Sync (loop) |
| `POST /tasks/promote` | promote ready tasks | Claim (loop) |
| `POST /tasks/:id/claim` | ready→running, mint agent token | Claim (loop) |
| `POST /tasks/:id/heartbeat` | liveness ping | Heartbeat (agent) |
| `POST /tasks/:id/gate` | running→gating | Claim (loop) |
| `POST /tasks/:id/done` | gating→done (commit) | Done |
| `POST /tasks/:id/block` | *→blocked (reason) | Block |
| `GET /engine` | hcom availability (installed? version) | — |
| `GET /agents` | agent CLIs: installed? key set? ready? | Secret (loop) |
| `GET /secrets` | stored credential metadata (no values) | Secret (loop) |
| `PUT /secrets/:tool` | seal + store an agent CLI credential | Secret (loop) |
| `DELETE /secrets/:tool` | remove a stored credential | Secret (loop) |
| `GET /secrets/env` | decrypt all secrets → env pairs (spawn) | Secret (loop) |
| `GET /extensions` (`?enabled=1` `?frontend=1`) | list installed extensions | — |
| `GET /extensions/:id` | one extension (manifest + grants) | — |
| `POST /extensions` (`?id=`) | install a component (upload / URL) | Extension |
| `DELETE /extensions/:id` | uninstall | Extension |
| `POST /extensions/:id/grants` | set granted capabilities (`granted ⊆ requested`) | Extension |
| `POST /extensions/:id/enable` · `/disable` | toggle (installs land disabled) | Extension |
| `POST /extensions/:id/invoke` | run one exported extension-point under the sandbox | Extension |
| `GET /extensions/:id/frontend/*path` | proxy an enabled remote's federated bundle | — |

## Extensions

Two planes, one installed unit (design: [docs/design/extension-system.md](docs/design/extension-system.md);
worked example: [examples/gate-verdict-tab](examples/gate-verdict-tab)):

- **Backend (WASM).** Extensions are WebAssembly components hosted by
  `lazybones-ext` (Wasmtime, Component Model / WASI Preview 2). Each guest runs
  under a fuel + epoch + memory + timeout sandbox with **default-deny**
  capabilities — host services (read-only task/run views, an `http`-fetch
  allowlist, `log`) are reachable only when explicitly granted, and never wider
  than the manifest requested. Extension points (`gate-check`, event-reaction)
  are keyed by exported WIT interface and dispatched with a fail-open/closed
  policy, a per-extension circuit breaker, and an event-cycle guard.
- **Frontend (Module Federation).** An extension may ship a federated remote.
  The host (`ui/`) fetches enabled remotes from `GET /extensions?frontend=1` on
  boot, registers each `remoteEntry.js` with the Module Federation runtime, and
  mounts it through `@lazybones/ext-sdk` (`ui/packages/ext-sdk`) into named UI
  slots (e.g. `task-detail.tab`). Everything the remote touches goes through the
  SDK — it never reaches around it to the daemon directly.

The unit of identity is an embedded `lazybones.ext.toml` manifest (a custom
section the daemon reads on install): it declares the `wit-world`, the exported
`extension-points`, the requested `capabilities`, and an optional `[frontend]`
remote. Installs land **disabled with no grants**; an admin reviews, grants, and
enables.

## Engine + agent setup (the secret store)

lazybones runs an agent CLI per task (`agent_tool`: claude | codex | gemini |
opencode). `GET /engine` reports whether the **hcom** engine is installed;
`GET /agents` reports, per tool, whether the **CLI** is installed and whether a
**credential** is set. Keys are registered through the app (or `PUT /secrets/:tool`)
and **encrypted at rest** (AES-256-GCM) under a master key — `LAZYBONES_SECRET_KEY`
(falls back to the loop token; override it for any real run, and note that changing
it makes existing secrets undecryptable). The DB never holds a plaintext key; only
the loop-guarded `GET /secrets/env` decrypts them, which the loop exports into each
agent's environment at spawn. Listing only ever returns a `…last4` hint.

## Config

`lazybones.yaml`, every key overridable by `LAZYBONES_*` env. The daemon reads
only its boot keys (bind, data dir, namespace/database, run label, loop token);
the gate/concurrency/worktree keys are consumed by the in-process scheduler.

## Not yet built (tracked follow-ups)

- **AI memory** — `POST /memory` + `GET /memory/recall` (vector + graph recall).
  The `memory` table is declared; the embedding provider is an open question
  (SCOPE.md OQ7), so the routes are deliberately deferred.
- **`GET /stream`** — SurrealDB live-query → SSE status feed.
- **The scheduler** — an in-process Rust loop in `lazybonesd` (`src/scheduler/`)
  plus a typed hcom CLI client (`src/hcom/`): read ready Tasks → worktree → spawn
  via `hcom` → await the DONE event → gate → advance. This is the only execution
  plane and the top priority; nothing runs until it exists. (Not a shell script —
  see [docs/vision.md](docs/vision.md).)
