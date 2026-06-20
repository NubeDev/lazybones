# Plans and Runs: reusable recipes, real hcom execution

> Status: proposal, not yet built.
> Audience: anyone working on Plan authoring, the board UI, the API/store model,
> or the `hcom` adapter loop.
>
> Terminology and the layering rationale live in [Vision](vision.md). This doc is
> the concrete spec; it inherits the **Plan / Run / Task** names from there. (The
> earlier draft called these "workflow template / workflow run / task instance" —
> renamed to stop colliding with hcom's own "workflow scripts".)

This document extends [Starting tasks](starting-tasks.md). That doc explains the
current handoff:

```text
pending --promote (UI)--> ready --claim (scheduler)--> running
```

The same split must stay true at the Run level. The UI can author and start a
Run, but it must not pretend an agent is running. `hcom` remains the execution
plane.

## TL;DR

lazybones currently has one main durable noun: **task**. That is too small for
reusable work.

We need three nouns (see [Vision § Terminology](vision.md)):

- **Plan** — a reusable recipe, such as "normal PR review". Not executable by
  itself.
- **Run** — one instantiation of a Plan with inputs, such as "review PR 123 in
  repo X".
- **Task** — the existing executable unit the `hcom` adapter loop claims and runs.

The database is the source of truth for all three. We should **not** add workflow
YAML files as the normal authoring/runtime path. YAML may exist as an optional
import/export convenience, but the app should work without any workflow YAML at
all.

## Why the current task model is not enough

A task is good for execution:

- it has lifecycle state;
- it can be claimed by the loop;
- it owns paths;
- it gets a worktree/branch/session;
- it is gated and marked done or blocked.

But a task is bad for reusable intent. If "normal PR review" is stored as a
single task, then the structure is trapped inside prose. The UI cannot show the
review phases, hcom cannot run the safe parts in parallel, and a user cannot reuse
the same review plan without copying text.

The missing layer is a Plan that expands into Tasks.

## Names and boundaries

`hcom` already uses "workflow" to mean a runnable script discovered by `hcom run`.
We do not reuse the word. lazybones' nouns are **Plan / Run / Task** (full table in
[Vision § Terminology](vision.md)). That gives a clean, collision-free contract:

```text
Plan
  -> instantiate with inputs
  -> Run
  -> Tasks
  -> the one hcom adapter script claims/runs Tasks
```

## No workflow YAML as the source of truth

The earlier scope allowed `workfile.yaml` as a seed format. That made sense while
lazybones was bootstrapping a queue. It should not become the main workflow
design.

For Plans and Runs:

- The DB is authoritative for Plans, Runs, Tasks, status, history, inputs,
  outputs, and memory.
- The UI authors templates and starts runs through the API.
- The hcom script reads executable work from the API, not from a YAML file.
- YAML/JSON export can exist for backup, sharing, examples, or migration.
- YAML import can exist for "paste this starter template", but after import the
  DB owns the record.

In other words: **files are optional transport; the DB is the product**.

This matters because Runs are living state. A Run accumulates claims, heartbeats,
blocked reasons, commits, hcom session names, memory, user edits, and history. A
YAML file cannot be the truth for that without becoming another database, badly.

## Starting a Run

Starting a Run is not the same as starting an agent. It is a higher-level
control-plane action:

```text
Plan --instantiate (UI/API)--> Run
Run  --activate    (UI/API)--> pending Tasks
Task --promote     (UI/API)--> ready
Task --claim       (hcom adapter)--> running
```

Recommended user-facing wording:

- **Create Run**: instantiate a Plan and create its Tasks.
- **Start Run**: activate the Run and promote all eligible root Tasks to `ready`.
- **Running**: the daemon (hence the scheduler) is up and the hcom CLI is available,
  so ready Tasks get claimed.

Do not use "Start" to mean `claim`. The UI should never call `/tasks/:id/claim`.
That rule from [Starting tasks](starting-tasks.md) still holds.

## Data model

### `plan`

Reusable recipe. No lifecycle state.

Suggested fields:

```text
id: string
title: string
description: string
version: integer
inputs: input_def[]
task_defs: task_def[]
defaults:
  agent_tool?: string
  worktree_mode?: new | reuse | branch
  gate_profile?: string
created_at
updated_at
archived_at?
```

`task_def` is a task recipe, not a Task:

```text
id: string
title: string
spec_template: string
deps: string[]
owns_template?: string[]
tool?: string
worktree_mode?: new | reuse | branch
optional?: boolean
```

### `run`

Concrete run of a Plan.

Suggested fields:

```text
id: string
plan_id: string
plan_version: integer
title: string
lifecycle: active | paused | cancelled    # human-set only
state:     derived                         # see below
inputs: object
created_at
started_at?
finished_at?
```

The Run snapshots the Plan version so a later Plan edit does not rewrite history.

**Run state is derived, not independently set.** Only `lifecycle`
(`active | paused | cancelled`) is a human/API decision. The user-facing *state* is
computed from the Run's Tasks so it can never lie about reality:

```text
cancelled         if lifecycle = cancelled
paused            if lifecycle = paused
done              if every Task is done
needs-attention   if any Task is blocked
running           if any Task is running/gating
ready             if any Task is ready (and a loop could claim it)
draft             otherwise (no Tasks promoted yet)
```

This avoids a `run.status` field that drifts from the Tasks underneath it — the
overlap between a Run-level `done/blocked` and a Task-level `done/blocked` was a
trap in the earlier draft.

### `task`

The existing task document remains the executable unit. Add enough metadata to
group generated Tasks under their Run:

```text
run_id?: string          # FK to the parent Run
plan_task_id?: string    # which task_def in the Plan this came from
```

**Relationships are the key; the dotted name is only a label.** Do not encode the
parent id into the Task's primary key (`pr-123-review.inspect` as the PK re-couples
truth to a string — the thing SCOPE principle 6 warns against). Use `run_id` +
`plan_task_id` as the real link, and render a human label for the board:

```text
pr-123-review · inspect      (label)
pr-123-review · review
pr-123-review · fix
pr-123-review · verify
```

Run-aware views key off `run_id`, never off parsing the label.

## API scope

Add Plan/Run routes without disturbing the task lifecycle routes:

| Method · path | Job |
| --- | --- |
| `GET /plans` | list reusable Plans |
| `POST /plans` | create a Plan |
| `GET /plans/:id` | fetch Plan detail |
| `PATCH /plans/:id` | edit a Plan, bumping version |
| `POST /plans/:id/instantiate` | create a Run and its Tasks |
| `GET /runs` | list Runs |
| `GET /runs/:id` | Run detail, generated Task ids, derived state, progress |
| `POST /runs/:id/start` | activate Run and promote eligible root Tasks |
| `POST /runs/:id/pause` | stop promoting new Tasks; running Tasks continue or are handled by policy |
| `POST /runs/:id/cancel` | block unclaimed Tasks and kill claimed agents via `hcom kill tag:<task-id>` |

> Note: `GET /runs/:id` here returns *Plan-run* detail. The existing
> `GET /runs/:id` (event history, README) is a different noun — fold both under the
> Run detail response, or namespace the event log as `GET /runs/:id/events`.

Existing task routes keep their meaning:

- `POST /tasks/:id/ready` promotes one task.
- `POST /tasks/:id/claim` is scheduler-only (the in-process Rust loop).
- `POST /tasks/:id/heartbeat` is agent-only.
- `POST /tasks/:id/gate` and `POST /tasks/:id/done` remain the green-build path.

## hcom integration — a Rust client, not a shell script

There is **no** `~/.hcom/scripts/lazybones.sh`. hcom's integration surface is its
**CLI binary**: `hcom run` is only sugar that execs a script which itself just calls
`hcom` subcommands. `lazybonesd` calls those subcommands directly via
`std::process::Command`, behind a typed Rust client (e.g. a `lazybones-hcom`
module/crate):

```rust
hcom.spawn(tool, &SpawnOpts { tag, dir, headless, prompt, .. })?; // hcom N <tool> --tag … --dir … --headless --hcom-prompt …
hcom.wait_event(&Filter::task_done(task_id), timeout)?;           // hcom events --wait --json --sql …
hcom.list()?;                                                     // hcom list --json
hcom.kill(Target::Tag(task_id))?;                                 // hcom kill tag:<task-id>
```

Baseline checked: `hcom 0.7.21` (CLI flags confirmed in source:
`--tag/--dir/--headless/--hcom-prompt/--name` on launch, `--thread` on send,
`--json` + `--wait --sql` on events, `kill tag:`). The public project is
[aannoo/hcom](https://github.com/aannoo/hcom).

The scheduler is a **Tokio task inside `lazybonesd`**. Per pass it:

1. Reads claimable Tasks straight from the store (no HTTP round-trip to itself).
2. Provisions the worktree per the Task's stored `worktree_mode`.
3. Marks the Task `running` (the internal equivalent of `/claim`).
4. Spawns the agent via the hcom client with `--tag <task-id>` (the kill/observe
   handle), `--dir`, `--headless`, and a generated prompt.
5. Awaits the Task's DONE/blocked signal via `hcom events --wait --json` (no
   `sleep`); agents `POST /tasks/:id/heartbeat` meanwhile.
6. Runs the gate, pushes/merges where configured, advances the Task to `done` or
   `blocked`.
7. Repeats until no Run has claimable or running work.

hcom owns agent process spawning; lazybones owns durable state and every scheduling
decision — in Rust. See [Vision § 2/3](vision.md) for the events, collision, and
kill/fork/resume surfaces the same client should project into the API/UI.

**Concurrency.** A single global concurrency budget across all active Runs, with
per-Run fairness (round-robin claim) — not `concurrency` applied per Run, which
would over-spawn N× when N Runs are active.

## Starting a Run (no CLI loop to launch)

Because the scheduler lives in the daemon, there is nothing extra to start. A Run
begins executing the moment it is activated over the API:

```sh
curl -X POST localhost:46787/runs/feat-checkout/start   # activate → scheduler picks it up
```

There is no `hcom run lazybones`, and **no `--plan … --input …` form**:
instantiation goes through `POST /plans/:id/instantiate` (one code path, DB stays
the product). An optional `lazybonesd run <id>` thin CLI may exist for headless/CI,
but it just calls the same API the daemon already serves.

## UI scope

Add a top-level **Plans** view above **Tasks**:

```text
Dashboard
Plans
Tasks
Run history
Settings
```

### Plans view

Show reusable Plans and recent Runs:

- Plan name;
- latest version;
- number of task definitions;
- last Run;
- success/block rate;
- "New Run" action.

### Run detail

Show the Run as a plan graph, not only as lifecycle columns. Because the
dependency graph is first-class, draw the real shape (fan-out included):

```text
scaffold
  ├─ module-a ─┐
  ├─ module-b ─┼─ integrate ── verify
  └─ module-c ─┘
```

Provide tabs:

- **Plan** — dependency graph / ordered phases.
- **Board** — existing status board filtered to this Run.
- **Events** — hcom message/status/life stream for the Run's tagged agents,
  persisted (see [Vision § 2](vision.md)).
- **Memory** — decisions, gotchas, follow-ups emitted by Tasks (`POST /memory`),
  fused with hcom handoff messages.

### Task board

Keep the existing board. It is still useful for execution debugging. Add a Run
filter and a link back to the parent Run.

## Example: a parallel build (proves the graph)

A pure chain (`inspect→review→fix→verify`) is the case where dependencies buy
nothing. Use a **fan-out** example so the graph and hcom's collision guard earn
their place.

Plan:

```text
id: feature-build
title: Build a feature across modules
inputs:
  repo
  spec?
task_defs:
  scaffold                       deps: []
  module-a    deps: [scaffold]   owns: [src/a/**]
  module-b    deps: [scaffold]   owns: [src/b/**]
  module-c    deps: [scaffold]   owns: [src/c/**]
  integrate   deps: [module-a, module-b, module-c]
  verify      deps: [integrate]
```

Run:

```text
id: feat-checkout
plan: feature-build@1
inputs:
  repo: owner/project
```

Generated Tasks (label · concept; keyed by `run_id`):

```text
feat-checkout · scaffold
feat-checkout · module-a     ┐
feat-checkout · module-b     ├─ run in parallel, three worktrees, three agents
feat-checkout · module-c     ┘
feat-checkout · integrate
feat-checkout · verify
```

`module-a/b/c` claim concurrently into isolated worktrees; the loop subscribes
`hcom events sub --collision` as a second safety net behind `owns`. Only Tasks are
claimed by hcom.

## Makefile scope

`make dev` starts daemon + UI — and because the scheduler lives **inside** the
daemon, that is also the loop. There is no separate loop process to launch.

```make
make dev      # daemon (incl. scheduler) + dashboard
make status   # daemon, UI, hcom CLI availability
```

The "loop connected / idle" confusion from [Starting tasks](starting-tasks.md)
disappears: if the daemon is up, the queue is being consumed. The only remaining
health signal worth surfacing is whether the **hcom CLI** is installed/usable
(`GET /engine`), since the scheduler shells out to it.

## Non-goals

- Do not reimplement hcom's agent/PTY spawning; invoke the hcom CLI from Rust.
- Do not put the loop in a shell script; the scheduler is a Rust task in `lazybonesd`.
- Do not make YAML files the runtime source of truth (or the authoring path).
- Do not let the UI claim Tasks directly.
- Do not store mutable Run state in files.
- Do not force every Run to be linear; dependency graphs are first-class.
- Do not add a `--plan … --input …` CLI form in v1; instantiate via the API.

## Open questions

- Should Runs support branching conditions, or are optional Tasks enough for v1?
- On cancel, kill running agents immediately (`hcom kill tag:<id>`) or let them
  reach a gate first? (Mechanism is settled; the *policy* is open.)
- Should Plan edits always create a new version, or be mutable until first Run?
- Should Run-local memory be promoted to Plan-level knowledge after a successful
  Run? (Ties into the deferred memory routes — see [Vision § 2](vision.md).)
- Resolved from the earlier draft: Task ids do **not** encode the Run id; the
  `run_id` FK is the link and the dotted name is only a label.
