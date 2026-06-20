# Workflow supervisor — a per-workflow watcher that reports, then (later) acts

> Status: spec, ready to build. Backend-first. **Phase 1 is observe-and-report
> only.**
> Audience: whoever implements the supervisor layer.
> Read [workflows-scope.md](workflows-scope.md) for the Template / Workflow
> (= Run) / Task nouns and [scheduler.md](scheduler.md) for the tick loop. This
> doc adds **one new actor** on top of that model: a supervisor agent attached to
> a workflow.

## The user story this exists to serve

> 1. I build a workflow with a few tasks and start it.
> 2. Instead of watching the board, I open the workflow's **Status** feed and see
>    short, human-readable updates appear — "task-1 landed, task-2 running, looks
>    on track" — left by a **supervisor agent** that wakes after each task
>    finishes and on a timer.
> 3. I can hit **Run supervisor now** to get a fresh read on demand.
> 4. **(Later — not this phase)** the supervisor doesn't just report: it can decide
>    a finished task came out wrong and send it back, tweak a spec, or add a
>    follow-up task.

Phase 1 delivers steps 1–3. Step 4 is explicitly deferred, and the data model is
shaped so it slots in without a reshape.

## What the supervisor is

A **supervisor** is a real hcom agent — spawned exactly like a task agent, through
the same [`Hcom::spawn`](../crates/lazybones-engine/src/hcom/spawn.rs) path — but
scoped to a **workflow**, not a task. It is given the workflow's title, its task
list with current statuses, the recent transition history, and the tail of its own
prior status entries, and asked to **post one short status line** back to the brain.
That line is appended to a per-workflow, append-only **status log** the user reads as
a feed.

It is **not** the scheduler, and it is **not** engine-side string formatting. The
scheduler decides *when* the supervisor wakes; the supervisor agent decides *what to
say*. This matches Principle 1 ([SCOPE.md](../SCOPE.md)): hcom is the agent fabric,
lazybones is the brain — the brain schedules, the agent reasons.

## Decision: phase 1 is read-only

The supervisor in phase 1 **observes and reports**. It cannot redo a task, edit a
spec, add/remove tasks, or change any state. Its only write is appending a status
entry. This is deliberate:

- It is the smallest slice that proves the loop (wake → spawn → capture → display).
- A reporting agent that is wrong is harmless; an acting agent that is wrong creates
  runaway loops (redo → finish → wake → redo …). We earn trust before granting
  control.
- The status log it produces in phase 1 is exactly the substrate a phase-2 acting
  supervisor reviews. Nothing is throwaway.

The prompt instructs the agent, in phase 1, to **observe and report only — do not
modify the workflow or its tasks.**

## The nouns

### `supervisor_log` — append-only status entries (one per supervisor run)

A status entry is **not** a task transition. The existing
[`Event`](../crates/lazybones-store/src/event/row.rs) is a `from → to` status
change on a task; a supervisor status line is free text about the *workflow*. So it
gets its own small, append-only record, modelled on the event store
([append.rs](../crates/lazybones-store/src/event/append.rs) /
[history.rs](../crates/lazybones-store/src/event/history.rs)):

```text
run_id: string        # the workflow this entry belongs to (FK, like list_run_tasks)
text:   string        # the short status line — meant to be glanceable, log-sized
actor:  string        # "supervisor" (room for "supervisor:<tool>" later)
at:     string        # RFC3339
```

Kept small on purpose: the value is a scannable feed, not an essay per entry.

### supervisor bookkeeping on `run`

The architecture forbids in-memory cross-tick state — "every tick rebuilds reality
from the store" ([scheduler.md](scheduler.md), [run.rs](../crates/lazybones-engine/src/scheduler/run.rs)).
So *when the supervisor is due* must be persisted on the
[`Run`](../crates/lazybones-store/src/run/model.rs). Three new fields, all additive
(SCHEMALESS table — existing rows deserialize the new fields as defaults, no
migration), mirroring how `started_at` already lives there:

```text
supervisor_last_run_at?:   string   # RFC3339; None = never run
supervisor_wake_requested: bool      # default false; set by the finish-hook and the
                                     #   manual button, cleared when the supervisor runs
supervisor_interval_secs?: u64       # per-workflow timer; None = engine default
```

## When the supervisor wakes

Three triggers, all resolved in the tick by reading the run's persisted fields — no
new long-lived state:

1. **After a task finishes.** In [finish.rs](../crates/lazybones-engine/src/scheduler/finish.rs)
   `drive()`, once a task reaches `done` or `blocked`, if `task.run_id` is set, call
   `store.request_supervisor_wake(run_id)` — which only flips
   `supervisor_wake_requested = true`. The finish path does **not** spawn the
   supervisor itself; it raises a flag the tick acts on, so all spawning stays in
   one place (mirrors how claiming, not finishing, owns task spawns).
2. **On a timer.** Default 5 minutes (`supervisor_interval_secs`, engine-config
   default, per-workflow override). The tick wakes a run whose
   `now − supervisor_last_run_at ≥ interval` while the workflow is active.
3. **Manual.** `POST /workflows/:id/supervisor/wake` sets the same flag; the next
   tick picks it up.

### The new tick phase

[tick.rs](../crates/lazybones-engine/src/scheduler/tick.rs) `tick()` gains a fourth
phase after `claim_and_spawn`:

```text
reclaim → promote → claim_and_spawn → supervise
```

`supervise` iterates **active** runs and, for each that is due (flag set OR timer
elapsed):

1. clears `supervisor_wake_requested` and stamps `supervisor_last_run_at = now`
   **before** spawning — so a crash mid-spawn doesn't wedge a run into perpetual
   re-wake, and a concurrent tick won't double-spawn;
2. spawns the supervisor agent (tag `sup:<run_id>`) in a detached task, like
   `finish::drive` is spawned per claim, then walks away — the agent reports back by
   **POSTing its status line** (see "Capture" below), so there is no long-lived await
   per supervisor run.

Supervisor spawns count against the same global `concurrency` budget so they never
starve task work; if the budget is full this tick, the run stays flagged and is
picked up next tick.

**The stamp+flag-clear is the only re-spawn guard.** Stamping
`supervisor_last_run_at` and clearing `supervisor_wake_requested` before spawning
already prevents a second spawn (the run is no longer "due"). We deliberately do
**not** add an `hcom list` check for a live `sup:<run_id>` agent: it would shell out
once per active run per tick to defend against a case the bookkeeping already covers.
The cost of the rare double-spawn (a duplicate status line) is far below a per-tick
process call across every workflow.

### Capture — the agent POSTs its line

The supervisor reports by calling `POST /workflows/:id/supervisor` with its status
text — the same shape agents use to POST heartbeats — **not** by emitting a marker on
an hcom thread that a detached task blocks on. This is chosen over the
`DONE`/`BLOCKED`-style marker convention deliberately:

- **No long-lived await.** The marker path needs a detached task blocking on
  `hcom.wait()` for the whole supervisor run. Worse, the obvious timeout to reach for
  is `finish.rs`'s `AWAIT_SECS = 3600` — wrong here: a summarizer that hasn't spoken
  in a couple of minutes is a dead agent, not "real work running for minutes." The
  POST removes the await (and the timeout question) entirely.
- **Restart-robust.** A POSTed line is a durable row the instant it lands, so a daemon
  restart can't lose it. The marker path loses any in-flight awaiting task on restart.
- **It's the phase-2 path anyway.** The acting supervisor must call back into the API
  to redo tasks / edit specs (see the seam below). Building phase 1 on the POST means
  not building a marker mechanism we'd replace. Reuse of existing await plumbing is a
  weaker argument than not shipping a throwaway.

A supervisor agent that never POSTs simply leaves a stamped `supervisor_last_run_at`
and no entry; the next interval runs a fresh one. No await to time out, no slot held.

## Engine config

New keys, wired like the existing `agent_tool`
([config.rs](../crates/lazybones-engine/src/config.rs)) — yaml + `LAZYBONES_*`
env + default:

```text
supervisor_enabled:      bool   default true     # LAZYBONES_SUPERVISOR_ENABLED
supervisor_interval_secs: u64   default 900      # LAZYBONES_SUPERVISOR_INTERVAL_SECS
```

The default interval is **15 minutes**, not 5. The finish-trigger does the real work
— it fires at every interesting moment (a task landing or blocking). The timer exists
only to catch **stalls** (a task running long with nothing finishing), so a chatty
5-minute tick would mostly produce "still running" noise. 15 min is a stall-catcher.

The agent that runs the supervisor uses the global `agent_tool` by default. (If the
per-entity agent/model/effort work lands, a workflow's supervisor can carry its own
tool/model/effort; until then it is the global tool. A summary job is a natural fit
for a cheaper/faster model later.)

## REST surface

| Method · path | Job |
| --- | --- |
| `GET /workflows/:id/supervisor` | the status feed — `Vec<SupervisorEntry>`, newest first |
| `POST /workflows/:id/supervisor` | the supervisor agent posts its status line (the capture path) |
| `POST /workflows/:id/supervisor/wake` | flag the supervisor to run on the next tick; returns the workflow summary |

The wake route mirrors the cancel route
([workflows_cancel.rs](../crates/lazybones-api/src/routes/workflows_cancel.rs)):
capability check, mutate the run, return `WorkflowSummary`. The status-post route is
the agent's callback — in phase 1 it accepts only a status line and writes one
`supervisor_log` row; phase 2 widens what the supervisor may call (under its own
capability — see the seam). Optionally `supervisor_interval_secs` is exposed on the
workflow create/update DTO.

## UI

- A new **Status** tab on the workflow detail page
  ([workflow-detail.tsx](../ui/src/features/workflows/workflow-detail.tsx)), beside
  Events, rendering the supervisor feed — modelled on
  [workflow-events.tsx](../ui/src/features/workflows/workflow-events.tsx), reusing
  the event-row styling. Polled with a `refetchInterval` so it feels live.
- A **Run supervisor now** button in
  [workflow-controls.tsx](../ui/src/features/workflows/workflow-controls.tsx),
  wired to a `useTriggerSupervisor()` mutation on the wake route.
- TS types + api client for `SupervisorEntry` and the two endpoints.

## File layout (verb-per-file, ≤400 lines)

```text
crates/lazybones-store/src/supervisor/
  mod.rs            # barrel
  model.rs          # SupervisorEntry
  row.rs            # SurrealDB row + conversion
  append.rs         # append_supervisor_status(run_id, text, actor)
  history.rs        # supervisor_log(run_id) -> Vec<SupervisorEntry>
  # run bookkeeping (request_supervisor_wake / mark_supervisor_ran) lives with run/
crates/lazybones-engine/src/scheduler/
  supervisor.rs     # compose the prompt, spawn, capture the STATUS line
  # tick.rs gains the `supervise` phase; finish.rs gains the wake-on-finish hook
crates/lazybones-api/src/routes/
  workflows_supervisor.rs   # GET feed + POST wake
ui/src/features/workflows/
  workflow-supervisor-log.tsx   # the Status feed
```

## Task lifecycle — unchanged

The supervisor adds **no** new task states and does not touch the task state machine
([SCOPE.md](../SCOPE.md) "Task lifecycle"). `done` is still earned by commit + push +
green gate. In phase 1 the supervisor only reads that machine and narrates it.

## Restart & recovery

Consistent with Principle 3 — no in-memory supervisor state. On restart the tick
reads each run's `supervisor_last_run_at` / `supervisor_wake_requested` and resumes:
a run past its interval wakes, a flagged run wakes, everything else waits. Because
the agent reports by POST, a status line is a durable row the instant it lands —
there is no in-flight await to lose on restart. A supervisor agent that died before
posting leaves a stamped `supervisor_last_run_at` and no log entry; the next interval
simply runs a fresh one. Status entries are durable rows, so the feed survives any
restart.

## Non-goals (this phase)

- **No acting supervisor.** No redo, no spec edits, no add/remove tasks, no state
  changes beyond appending a status entry. That is phase 2.
- **No new task states** and no change to the gate or the done-definition.
- **No engine-side fake status.** The status line is written by a real agent, not
  formatted by the scheduler. (The scheduler decides *when*, never *what*.)
- **No per-supervisor history beyond the log.** One append-only feed per workflow;
  no separate supervisor "sessions" object.
- **No cross-workflow supervisor.** A supervisor watches exactly one workflow, keyed
  on `run_id` — the same FK `list_run_tasks` uses.

## The seam we leave for phase 2 (acting supervisor)

Everything above is shaped so granting the supervisor power is **additive**:

- The wake/spawn/capture loop already runs the agent on the right triggers — phase 2
  changes only what the agent is *allowed to do*, not when it runs.
- The capture path is already a POST back into the API. Phase 2 widens the prompt and
  lets the supervisor call additional endpoints (e.g. `Transition::Reclaim` to redo a
  task, or `add_workflow_task`) instead of only writing a status line.
- The status log written in phase 1 becomes the audit trail of phase-2 decisions.

**But "same REST API the UI uses" is not the same as "free."** Phase 2 hands an LLM
agent the ability to mutate workflows and redo tasks, which reopens the exact
runaway-loop risk that justifies phase 1 being read-only (redo → finish → wake →
redo). The HTTP layer is shared, but two new things are *not*, and must be built with
phase 2 — they are called out here so the report-only design isn't mistaken for a
free upgrade path:

1. **A scoped supervisor capability.** The supervisor token must carry its own authz
   scope (what a supervisor may mutate), distinct from an operator's. It is not the
   same credential the UI holds.
2. **Loop-breaking.** A hard cap on supervisor-initiated redos per task (and/or per
   workflow per window), so a supervisor that keeps rejecting the same output can't
   spin forever. Without this, the very failure mode phase 1 avoids returns.

**We are not building phase 2 now** — it is recorded so the report-only design
doesn't paint us into a corner, and so the trust boundary is on the table before
anyone wires the acting path.

## Resolved decisions

- **Capture protocol → POST.** The supervisor POSTs its status line; no hcom-thread
  marker, no long-lived await. See "Capture" above for the full rationale.
- **Interval → keep it, default 15 min.** The finish-trigger covers the interesting
  moments; the timer is purely a stall-catcher, so a short tick is noise. See
  "Engine config".
- **Lifecycle → active, non-terminal only.** The supervisor does not run for a
  `draft` workflow or keep ticking on `done`/`cancelled`; it writes one final entry on
  reaching `done`.

## Open questions

1. **Wake coalescing.** If three tasks finish within one tick, the flag yields one
   supervisor run (good — one summary). But timer + flag could still wake twice in
   quick succession. Is a minimum gap between supervisor runs (a floor regardless of
   trigger) worth it to keep the feed from getting noisy?
2. **Default tool/model for the supervisor.** Global `agent_tool` now. Worth pinning
   a cheaper/faster model by default once per-entity agent config exists, since this
   is a summarization job?
