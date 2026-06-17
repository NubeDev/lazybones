# task: store — embedded SurrealDB store boundary + health

## Goal
Own the durable read/write boundary for a run: open the embedded, file-backed
SurrealDB engine (in-memory for tests), bootstrap the namespace/database, and run
idempotent schema init. Expose a cloneable `StoreHandle` that the API drives.

## Deliverables
- `StoreEngine` (memory / file) + `open_engine`.
- `StoreHandle::open` → connect, `use_ns`/`use_db`, `init_schema`.
- A `/health`-able probe (`health()`).
- Task documents (`task` table), `depends_on` graph edges, `event` run-log rows.
- The lifecycle state machine (`Status::can_transition`) and `transition_task`,
  which writes the status change AND an `event` row atomically per call.
- Graph readiness query (`newly_ready`): pending tasks whose deps are all `done`.

## Done definition
- `cargo test -p lazybones-store` is green.
- A no-dep task becomes ready immediately; a dependent task only after its dep is
  `done`. An illegal transition (e.g. `pending → done`) is rejected.
- Re-import (`sync_seeds`) preserves a task's lifecycle (never resets to pending).
