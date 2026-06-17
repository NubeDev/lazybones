# task: api — REST surface (tasks, claim, heartbeat, done, block)

## Goal
The axum REST surface over the durable store — one file per route. Every mutating
route is guarded by a scoped session resolved from the request's bearer token.

## Deliverables
- `GET /health`, `GET /tasks` (`?status=`), `GET /tasks/:id`, `GET /runs/:id`.
- `POST /workfile/sync` (loop-only) — idempotent seed import.
- `POST /tasks/promote` (loop) — promote pending tasks whose deps are done.
- `POST /tasks/:id/claim` (loop) — ready→running, mints the agent's scoped token.
- `POST /tasks/:id/heartbeat` (agent) — liveness stamp.
- `POST /tasks/:id/gate` (loop) — running→gating before the gate re-run.
- `POST /tasks/:id/done` (agent/loop) — gating→done with the pushed commit.
- `POST /tasks/:id/block` — *→blocked with a reason.
- Error mapping: illegal transition → 409, missing task → 404, no/bad token → 401,
  missing capability or wrong task → 403.

## Done definition
- `cargo test -p lazybones-api` is green, covering the full lifecycle over HTTP,
  401 without a token, 409 on an illegal transition, and agent task-scoping.

## Follow-ups (not in this slice)
- `POST /memory` + `GET /memory/recall` — gated on the embedding-provider choice
  (SCOPE.md OQ7); the store already declares the `memory` table.
- `GET /stream` — SurrealDB live-query → SSE feed for dashboards + the loop.
