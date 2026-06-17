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
| `lazybones-cli` | `lazybonesd` — parse config, open the store, serve; plus a one-shot workfile import. |

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
curl localhost:7878/health
curl localhost:7878/tasks                       # list all
curl localhost:7878/tasks?status=ready          # filter by status
curl -X POST localhost:7878/tasks/promote \
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

## Config

`lazybones.yaml`, every key overridable by `LAZYBONES_*` env. The daemon reads
only its boot keys (bind, data dir, namespace/database, run label, loop token);
the gate/concurrency/worktree keys are consumed by the hcom loop script.

## Not yet built (tracked follow-ups)

- **AI memory** — `POST /memory` + `GET /memory/recall` (vector + graph recall).
  The `memory` table is declared; the embedding provider is an open question
  (SCOPE.md OQ7), so the routes are deliberately deferred.
- **`GET /stream`** — SurrealDB live-query → SSE status feed.
- **The hcom loop script** (`scripts/lazybones.sh`) — the orchestration loop that
  drives this API. lazybones is the queue + gate; the loop is hcom.
