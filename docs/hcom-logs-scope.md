# hcom logs — durable raw agent comms, traceable per agent, streamed live

> Status: spec, ready to build. Backend-first.
> Audience: whoever implements the hcom-log ingestion layer.
> Read [SCOPE.md](../SCOPE.md) for the principles and [scheduler.md](scheduler.md)
> for the tick loop. This doc adds **one new durable record** (`hcom_log`) and
> **one new live signal** on top of that model: the raw hcom event stream for a
> run's agents, persisted and pushed.

## The user story this exists to serve

> 1. A task finishes — or worse, blocks — and I want to know *what the agent
>    actually did and said*, not just that it went `running → blocked`.
> 2. Today I get the **run history**: a clean list of status transitions
>    (`GET /runs/:id`). That tells me the *shape* of the run, never the *content*.
>    To see the agent's messages I have to shell into `hcom transcript <name>` on
>    the box, and once the agent is reaped that's gone.
> 3. I want the **raw hcom logs** kept alongside run history — every message,
>    status, and lifecycle event the agent emitted — keyed to the task and run, so
>    I can trace one agent end-to-end after the fact.
> 4. And I want them **live over SSE**, so the UI can later show the agent's
>    messages streaming in as they happen, the same way the board shows transitions.

## Two logs, one run — and why they stay separate

We already have a durable run log: the [`Event`](../crates/lazybones-store/src/event/row.rs)
table, one row per `from → to` status transition, replayed by
[`GET /runs/:id`](../crates/lazybones-api/src/routes/runs.rs). That is the
**brain's** record — lazybones' own decisions about a task's lifecycle.

What's missing is the **fabric's** record — what hcom observed the agent doing:
its messages, its `listening/active/blocked` status pings, its
`started/stopped/killed` lifecycle. Principle 1 ([SCOPE.md](../SCOPE.md)): hcom is
the agent fabric, lazybones is the brain. The two logs answer different questions
and must not be merged:

| | Run log (`event`) | hcom log (`hcom_log`) — **new** |
| --- | --- | --- |
| Author | lazybones (the scheduler/gate) | hcom (the agent fabric) |
| Granularity | one row per lifecycle transition | one row per raw agent event |
| Question it answers | "what did the orchestrator decide?" | "what did the agent actually do/say?" |
| Source | `StoreHandle::transition` | `hcom events --json` (ingested) |
| Today | durable, REST + SSE | ephemeral, only via `hcom transcript` on the box |

An `Activity` ([activity.rs](../crates/lazybones-store/src/event/activity.rs)) is
the agent's *self-reported* progress note, pushed live but **never persisted** — a
signal, not history. The hcom log is the opposite: hcom-observed and **durable**.
The hcom log subsumes what activities gesture at, with the full record behind it.

## Decision: ingest, don't reimplement

We do **not** read hcom's SQLite directly, and we do **not** mirror the whole
transcript. We reuse the **exact surface the scheduler already trusts** —
`hcom events --wait <secs> --sql "<WHERE over events_v>"`
([hcom/events.rs](../crates/lazybones-engine/src/hcom/events.rs)), which prints one
JSON object per event (`id`, `ts`, `type` = `message | status | life`, `instance`
= agent name, `data`) and a `{"timed_out": true}` sentinel on no-match. The only
thing we add is a **non-blocking** form of it. This keeps Principle 1 intact: we
invoke hcom through the path we already have, parser and all
([`parse_events`](../crates/lazybones-engine/src/hcom/events.rs)), rather than
speccing a second CLI shape.

The non-blocking tail is just `--wait 0 --sql "id > {cursor}"` (the `--sql` clause
the existing `wait()` already uses, with a `0`-second timeout so it returns
immediately with whatever's queued). `Hcom::events_since(cursor, tags)` is then a
few lines on top of the existing command builder, returning `Vec<HcomEvent>` and
dropping the timeout sentinel exactly like `wait()` does. (0.7.21 also exposes
`--after <ts>`/`--last N`, but we deliberately do **not** use them: a timestamp
boundary is the source of the gap/re-pull worry, whereas `id >` is an unambiguous
monotonic-int comparison — see the cursor note below.)

The full conversation (tool I/O, file edits, assistant prose) stays where it lives,
behind `hcom transcript <name> --json`. The hcom log records the **event spine**
(messages + status + lifecycle) and keys it to our run/task; deep transcript
retrieval is an on-demand passthrough (see REST below), not a bulk copy.

## The nouns

### `hcom_log` — append-only raw agent events (one row per hcom event)

Modelled on the event store ([append.rs](../crates/lazybones-store/src/event/append.rs)
/ [history.rs](../crates/lazybones-store/src/event/history.rs)), but recording what
hcom saw rather than what we decided:

```text
run:        string        # the workflow this event belongs to (FK, like event.run)
task:       string?       # the task, when the agent's tag maps to one; None for run-scoped agents
agent:      string        # hcom instance name (the transcript handle)
tag:        string?       # the hcom --tag the agent launched with (task id, or sup:<run_id>)
hcom_id:    int           # hcom's monotonic event id — the ingestion cursor & dedup key
kind:       string        # "message" | "status" | "life"
data:       object        # the raw hcom `data` payload, kept verbatim (jsonb)
at:         string        # RFC3339, from hcom's `ts`
```

`(run, hcom_id)` is unique — re-ingesting the same hcom event is an idempotent
upsert. With the id-based cursor (below) the tail never re-pulls an already-seen
event in the first place, so this uniqueness is belt-and-suspenders against a
crash-mid-tail double-write rather than the only thing saving correctness. `data`
is stored verbatim (the message text, the status+context, the life action+reason)
so nothing the agent emitted is lossily reshaped — but `data` is also the row's
size driver (open question 3). For a pathological multi-MB message we cap the
stored payload (proposed: truncate `data.text` past, say, 64 KiB with a
`truncated: true` marker; the full thing is still in hcom's transcript). The
wire/JSON projection (`HcomLogEntry`) leaks no SurrealDB types, exactly like
[`Event`](../crates/lazybones-store/src/event/row.rs).

### tag → task → run: how an event finds its home

hcom events carry `instance` (agent name) and we know the `--tag` we launched each
agent with ([spawn.rs](../crates/lazybones-engine/src/hcom/spawn.rs)): a task id, or
`sup:<run_id>` for a supervisor. The tail resolves an event to `(run, task)` by its
tag:

- tag is a known task id → `task = id`, `run = task.run_id`.
- tag is `sup:<run_id>` → run-scoped, `task = None`.
- tag is unknown (an agent we didn't launch) → **dropped**; the hcom log is scoped
  to lazybones-launched agents on this run, not every agent on the box.

The resolver reuses the same tag↔task mapping reclaim already relies on
([reclaim.rs](../crates/lazybones-engine/src/scheduler/reclaim.rs) matches `hcom list`
tags to tasks), so there is one notion of "which agent is whose".

> **Shared contract — the `sup:<run_id>` tag.** This resolver and
> [supervisor-scope.md](supervisor-scope.md) both encode that a supervisor agent is
> launched with tag `sup:<run_id>`. That string is now a contract between the two
> layers: the supervisor spawn writes it, this tail parses it. Neither side renames
> it unilaterally — change it in one place and run-scoped events silently fall to
> the "unknown → dropped" branch.

### ingestion cursor on `run`

Principle 3 forbids in-memory cross-tick state — "every tick rebuilds reality from
the store". So *how far we've tailed* must be persisted. One additive field on the
[`Run`](../crates/lazybones-store/src/run/model.rs) (SCHEMALESS table — existing rows
deserialize it as default, no migration), mirroring how `started_at` already lives
there:

```text
hcom_log_cursor?: u64   # highest hcom_id ingested for this run; None = nothing yet
```

The cursor is hcom's **event id**, not a timestamp — `HcomEvent.id` is documented
"monotonic in hcom's local db" ([events.rs](../crates/lazybones-engine/src/hcom/events.rs)),
so the tail's WHERE clause is `id > {cursor}`: a total order with an unambiguous
boundary, no two-events-share-a-`ts` edge case. On restart the tail resumes from
`hcom_log_cursor` and asks hcom for `id > cursor`; no backlog is lost as long as
hcom retains it, and nothing at or below the cursor is ever re-fetched.

> One thing to pin down before relying on the id as the cursor (it deserializes as
> `serde_json::Value` today, not a typed int): confirm hcom always emits an integer
> `id`, and that ids stay monotonic **across hcom process restarts**, not just
> within a session. If they reset, a global cursor would skip the post-restart
> events — verify against 0.7.21 and, if needed, fall back to `ts` with the
> `(run, hcom_id)` upsert covering the boundary.

## Ingestion: the tail phase

[tick.rs](../crates/lazybones-engine/src/scheduler/tick.rs) `tick()` gains a phase
after `claim_and_spawn`:

```text
reclaim → promote → claim_and_spawn → tail_hcom
```

Because hcom's event id is **global**, not per-run, `tail_hcom` does **one** drain
per tick, not N. It takes the **minimum** `hcom_log_cursor` across active runs and
pulls everything newer in a single shell-out, fanning the result out to each run by
resolved tag — cheaper and simpler than a pull per run, and the `(run, hcom_id)`
upsert already makes the fan-out safe:

1. `lo = min(hcom_log_cursor)` over active runs (treat `None` as 0);
2. `Hcom::events_since(lo)` → `hcom events --wait 0 --sql "id > {lo}"`, parsed by the
   existing `parse_events` (timeout sentinel dropped). Returns immediately with the
   queued backlog;
3. resolve each event's tag → `(run, task)`, drop unknowns and any run that isn't
   active;
4. `append_hcom_log` each resolved event (idempotent on `(run, hcom_id)`);
5. advance each touched run's `hcom_log_cursor` to the max `hcom_id` it ingested,
   **after** the rows are written — so a crash between write and cursor-bump only
   re-ingests (harmless, idempotent), never skips;
6. publish each new entry on the live bus (see SSE).

> **SQL interpolation.** Step 2 interpolates `lo` into the WHERE clause the same way
> [finish.rs](../crates/lazybones-engine/src/scheduler/finish.rs) interpolates its
> own values — safe here because `lo` is a `u64` we control, never agent input. If a
> later refinement narrows the pull by tag (`AND tag IN (…)`), note that the tags
> are lazybones-minted (task ids / `sup:<run_id>`), so that too is trusted — but say
> so at the call site, since it's the one place string values touch a query.

This is **best-effort and self-contained**, like every tick phase: a tail failure is
logged and never aborts the pass. The drain does no agent work and holds no await —
it's cheap, so it adds negligible cost to the tick.

> Why drain in the tick, not a long-lived `hcom events --wait` follower? Because
> Principle 3: a follower is in-memory cross-tick state a restart loses. The
> cursor-on-run + drain-per-tick pattern is restartable by construction and reuses
> the loop we already trust. Tick latency (seconds) is fine for a log; the *live*
> feel comes from SSE on top, not from the persistence cadence.

## SSE: the new live signal

The live bus ([bus.rs](../crates/lazybones-store/src/event/bus.rs)) already carries
two variants over the in-process broadcast that `/stream` drains. We add a third:

```rust
pub enum LiveEvent {
    Transition(Event),   // durable status change (existing)
    Activity(Activity),  // ephemeral progress note (existing)
    HcomLog(HcomLogEntry), // NEW: a raw hcom event, also durable
}
```

`tail_hcom` publishes each newly-ingested entry. [stream.rs](../crates/lazybones-api/src/routes/stream.rs)
`to_sse` gains a `hcom_log` named SSE event:

```text
event: hcom_log
data: { "run":"workflow-1","task":"task-2","agent":"kula","kind":"message", ... }
```

The browser's existing `EventSource` picks it up with no new connection. As with
transitions, a lagging client silently skips dropped items and recovers by
refetching the durable log — the SSE feed is the live edge, `GET …/hcom` is the
complete record. Because persistence happens *before* publish, anything streamed is
already durable; the UI never shows a message it can't later re-fetch.

## REST surface

| Method · path | Job |
| --- | --- |
| `GET /runs/:id/hcom` | the run's raw agent log — `Vec<HcomLogEntry>`, oldest first; `?task=<id>` filters to one agent, `?kind=message` filters by type, `?after=<hcom_id>&limit=<n>` pages |
| `GET /tasks/:id/hcom` | sugar for `GET /runs/:run/hcom?task=:id` — one agent's full trace |
| `GET /tasks/:id/transcript` | on-demand passthrough to `hcom transcript <agent> --json --full` for the deep conversation (tool I/O, file edits); not persisted, fetched live from hcom for a still-known agent |

The first two read the durable `hcom_log`, modelled on
[runs.rs](../crates/lazybones-api/src/routes/runs.rs) `run_history`. The transcript
route is a thin shell-out for when the event spine isn't enough and the agent is
still in hcom's retention — explicitly **not** a stored artifact (it can be large
and hcom owns it).

## UI (later — backend lands first)

- A **Logs** tab on the workflow detail page
  ([workflow-detail.tsx](../ui/src/features/workflows/workflow-detail.tsx)), beside
  Events, rendering the hcom log grouped by agent/task — modelled on
  [workflow-events.tsx](../ui/src/features/workflows/workflow-events.tsx). It seeds
  from `GET /runs/:id/hcom` and then appends `hcom_log` SSE events live, so the feed
  grows as the agent talks.
- Per-task drill-in: opening a task shows its `GET /tasks/:id/hcom` trace, with a
  "load full transcript" affordance hitting the transcript passthrough for the deep
  view.
- TS types + api client for `HcomLogEntry`, the new SSE event name, and the
  endpoints. The SSE client gains one `case "hcom_log"` beside `transition` /
  `activity`.

## File layout (verb-per-file, ≤400 lines)

```text
crates/lazybones-store/src/hcom_log/
  mod.rs            # barrel
  model.rs          # HcomLogEntry (wire projection)
  row.rs            # SurrealDB row + conversion
  append.rs         # append_hcom_log(entry) — idempotent upsert on (run, hcom_id)
  history.rs        # hcom_log(run, filters) -> Vec<HcomLogEntry>
  # cursor bookkeeping (read/advance hcom_log_cursor) lives with run/
crates/lazybones-store/src/event/
  bus.rs            # add LiveEvent::HcomLog variant
crates/lazybones-engine/src/hcom/
  events.rs         # add Hcom::events_since(cursor) -> Vec<HcomEvent> (--wait 0 --sql "id > {cursor}")
crates/lazybones-engine/src/scheduler/
  hcom_tail.rs      # the tail phase: drain, resolve tag→task, append, advance cursor, publish
  # tick.rs gains the `tail_hcom` phase
crates/lazybones-api/src/routes/
  hcom_log.rs       # GET /runs/:id/hcom + GET /tasks/:id/hcom
  transcript.rs     # GET /tasks/:id/transcript passthrough
  stream.rs         # add the hcom_log named SSE event
ui/src/features/workflows/
  workflow-hcom-log.tsx   # the Logs feed (later)
```

## Task lifecycle — unchanged

The hcom log is pure observation. It adds **no** task states, touches **no**
transition ([SCOPE.md](../SCOPE.md) "Task lifecycle"), and never gates anything.
`done` is still earned by commit + push + green gate. The log only records what the
agent did on the way there.

## Restart & recovery

Consistent with Principle 3 — no in-memory tail state. On restart the tick reads
each run's `hcom_log_cursor` and resumes the drain from there. A crash between
writing rows and advancing the cursor re-ingests a few events; the `(run, hcom_id)`
upsert makes that a no-op, so the log never duplicates. Durable rows mean the log
survives any restart; only events hcom itself has already reaped (past its
retention, before we tailed) are unrecoverable — the cursor keeps that window as
small as one tick.

## Non-goals (this phase)

- **No bulk transcript mirroring.** We persist the event spine (message/status/life),
  not every tool call and file diff. The deep transcript stays in hcom, reached
  on-demand. (Copying it wholesale would duplicate hcom's storage and bloat the DB.)
- **No log-driven decisions.** The scheduler does not read the hcom log to decide
  anything — claiming, finishing, and gating are unchanged. This is observability,
  not control. (A phase-2 supervisor *reading* the log to act is a separate seam,
  see [supervisor-scope.md](supervisor-scope.md).)
- **No retention/rotation policy yet.** The log grows append-only. Pruning old
  runs' logs is a follow-up once we see real volume.
- **No cross-run/global log view.** The log is keyed on `run` like every other
  durable record; a box-wide "all agents everywhere" view is hcom's own TUI, not
  ours.
- **No editing or redaction.** The log is what hcom emitted, verbatim. Secret
  scrubbing, if needed, is a later concern at the ingestion boundary.

## The seam this leaves open

- **Activities can fold into the log.** Today `Activity` is a separate ephemeral
  variant. Once the hcom log streams agent messages live, the self-reported
  progress note is largely redundant; a later pass can route activities through the
  same durable path or retire them. Not now — they ship independently.
- **The phase-2 acting supervisor** ([supervisor-scope.md](supervisor-scope.md))
  gets a durable, queryable record of what each agent did — exactly the substrate it
  needs to judge "did this task come out right?" without re-driving the agent. The
  log written here is that audit trail.

## Open questions

1. **Message vs. status/life volume.** The global drain pulls all three event kinds.
   Status pings (`listening`/`active`) can be chatty and low-value next to messages.
   Persist all kinds, or keep messages + life durable and treat status as
   stream-only (like today's `Activity`)? Proposal: persist all three first, measure,
   then demote status to stream-only if it dominates row count.
2. **What counts as `message` worth keeping.** hcom messages include agent-to-agent
   chatter and `@mentions`. For a single-agent-per-task run that's just the agent's
   own line, but supervisors and future multi-agent tasks make it richer. Keep all
   `message` events keyed to the run, or filter to the task agent's own? Proposal:
   keep all, tagged by `agent`, and let the UI filter.
3. **Retention.** Append-only forever, or a TTL / max-rows-per-run cap? Tie to run
   terminal state (prune a `done`/`cancelled` run's log after N days)? Deferred, but
   the `data` payloads are the size driver, so worth deciding before heavy use.
4. **Transcript passthrough liveness.** `GET /tasks/:id/transcript` only works while
   hcom still retains the agent. Should a terminal task snapshot its full transcript
   once (one durable blob) so the deep view survives reaping, or is the event spine
   enough and the deep view best-effort? Proposal: event spine durable, transcript
   best-effort, revisit if the deep view proves essential post-mortem.
5. **hcom id durability across restarts.** The id-based cursor (see "ingestion
   cursor") assumes `HcomEvent.id` is an integer that stays monotonic across hcom's
   own process restarts, not just within a session — and that it doesn't reset when
   hcom rotates/archives its db. Verify against 0.7.21; if ids can reset, fall back
   to a `(ts, id)` composite cursor with the `(run, hcom_id)` upsert covering the
   boundary. (This is the one unresolved correctness assumption in the design.)
```
