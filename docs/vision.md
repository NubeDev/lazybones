# Vision: lazybones is the durable brain over the full hcom surface

> Status: north star. Rarely changes. Specs point here, not the other way around.
> Audience: anyone deciding what lazybones should and should not become.

## One sentence

**lazybones is the durable, multi-user, web-reachable control plane and shared
memory over the _full_ hcom surface.** hcom is the agent fabric — spawn, message,
observe, react, kill, relay. lazybones makes all of it durable, queryable, gated,
and safe to drive from a browser, and adds the one thing hcom deliberately lacks:
persistent state and a green-build gate.

## Why this framing

hcom's design is "no background services, state in a local SQLite, CLI-driven."
That is its strength and its ceiling:

- single machine, single operator;
- ephemeral by default (the db is a working set, not a record);
- terminal-first (you drive it by typing commands).

lazybones' reason to exist is to be hcom's **missing durable brain**. We own what
hcom doesn't: durable state, a schedule, a gate, remote/multi-user access, and
growing memory. We _wrap_ everything hcom already does well — and the wrapper is
Rust, not bash.

A design that only wraps `hcom run` + `claim` is wrapping the thinnest possible
slice. hcom's real differentiator is agents **messaging, observing, and reacting to
each other**. The vision is to surface that, durably.

### Rust-native, DB-native — no shell script, no files as truth

The orchestration runs **inside `lazybonesd` (Rust)**, not in a bash script under
`~/.hcom/scripts/`. hcom's integration surface is its **CLI binary**: `hcom run` is
just sugar that execs a script which itself only calls `hcom` subcommands. So the
daemon calls those subcommands directly via `std::process::Command` — a typed Rust
client over `spawn` / `events --json` / `list --json` / `kill` / `fork`. No bundled
script, no `hcom run lazybones`, no `--name` forwarding, no parsing `Names:` out of
stdout.

Two consequences that delete whole problems:

- **The loop is the daemon.** "Is a loop connected?" stops being a question — if
  `lazybonesd` is up, the scheduler is running. (Resolves SCOPE OQ5.)
- **The DB is the only authoring surface.** Plans, Runs, Tasks, specs, and the
  queue live in SurrealDB and are created over the API/UI. YAML/markdown
  (`workfile.yaml`, `tasks/*.md`) become an **optional import**, not the path. The
  one irreducible file is boot config — bind address + where the DB lives — because
  you must know that *before* you can read the DB; everything else moves into it.

## The three layers we wrap

### 1. Schedule & gate — _mostly built_

The part hcom has no opinion on. lazybones owns it:

- **Plan / Run / Task** (see terminology below) — reusable intent, instantiated
  intent, executable unit.
- Dependency readiness, worktree isolation as the parallelism mechanism.
- `commit + push + green re-gate` as the **only** path to `done`. A red gate is
  `blocked`, never silently done.
- **The scheduler is a Rust task in `lazybonesd`** — it reads ready Tasks from its
  own store, provisions worktrees, spawns agents through the hcom CLI, and advances
  state. No external loop process.

### 2. Mirror & expose hcom's runtime — _the real gap_

lazybones observes hcom's event stream (`~/.hcom/hcom.db`, or `$HCOM_DIR`) and
projects it into durable, API-served, UI-visible state. hcom's copy is local and
ephemeral; ours survives restarts and is reachable over HTTP by more than one
person.

| hcom primitive | lazybones surface |
| --- | --- |
| `message` / `status` / `life` events for a run's tagged agents | **Events** feed, persisted per run |
| agent `POST /memory` notes + hcom handoff messages | **Memory**, vector-indexed, recalled by the next task |
| `hcom events sub --collision` | runtime collision guard (complements/replaces static `owns` globs) |
| `hcom events sub --file "*.rs"` / `--idle` | reviewer-reacts-to-builder, wake-on-stall |
| `hcom list --json` + `GET /engine` | **daemon up** (loop is inherent) + **hcom CLI available** signals |

### 3. Control hcom's lifecycle from the API/UI — _missing_

Because lazybones stores the hcom **tag** (`--tag <task-id>`) and session name per
task, these become first-class REST actions instead of terminal-only commands:

- **kill** a stuck agent — `hcom kill tag:<task-id>`
- **stop** gracefully — `hcom stop <name>`
- **fork** to investigate — `hcom fork <name>`
- **resume / reclaim** a crashed task — `hcom resume`, reconciled against
  `git worktree list` + `hcom list`.

## The acceptance test for the whole vision

> A user who never opens a terminal can author a Plan, start a Run, watch agents
> message each other and edit files, catch a collision, kill a stuck agent, read
> what the Run learned, and merge on green.

Everything hcom can do — made durable and remote. If a feature does not move us
toward that sentence, it is probably out of scope.

## Terminology (resolve the collision, don't footnote it)

"Workflow" is overloaded across three layers. We do **not** reuse the word; we pick
non-colliding names so every downstream sentence is unambiguous.

| Name | Owner | Meaning |
| --- | --- | --- |
| **hcom workflow script** | hcom | a shell/Python script in `~/.hcom/scripts/`, run with `hcom run <name>`. We install **none** — `lazybonesd` drives the `hcom` CLI directly. |
| **Plan** | lazybones DB | a reusable recipe — task definitions, dependencies, inputs, defaults. Not executable. (was "workflow template") |
| **Run** | lazybones DB | one instantiation of a Plan with concrete inputs; owns generated Tasks. (was "workflow run") |
| **Task** | lazybones DB + hcom | the existing executable unit the loop claims and runs. |

```text
Plan --instantiate--> Run --activate--> Tasks --claim (lazybonesd scheduler)--> running
```

## Hard boundaries (what we will not become)

- **Not a re-implementation of hcom.** hcom owns agent process spawning — PTYs,
  multi-tool launch, the messaging fabric. We *invoke* it; we don't rebuild it. The
  scheduling loop, however, **is ours and is Rust** — there is no shell script.
- **Not a file-as-truth system.** The DB is authoritative and is the authoring
  surface. YAML/JSON is optional import/export transport, never live state. Only
  boot config (bind + DB location) stays a file.
- **Not a UI that fakes execution.** The UI promotes to `ready`; it never calls
  `/claim`. A `running` task always has a real agent behind it.
- **Not agent-tool-locked.** Any hcom-supported tool (claude, codex, gemini,
  opencode, …) can run a Task, chosen per-Run or per-Task.

## What this implies for sequencing

The single most valuable artifact is **the in-process scheduler** — a Rust module
in `lazybonesd` that drives a typed hcom client (`spawn` / `events --json` /
`kill` / `list`). It is the only execution plane, and it unblocks the top limiters
at once: worktree intent goes obeyed (Rust reads the field), loop liveness is
inherent (the loop is the daemon), and Tasks can finally run. Build the scheduler +
the hcom client; the rest of the vision hangs off them.

The implementation-grade spec — crate layout, config, the hcom client API, the
tick state machine, worktree/gate/merge, and a test plan — is in
[scheduler.md](scheduler.md).

See [starting-tasks.md](starting-tasks.md) for the two-actor lifecycle and
[starting-workflows.md](starting-workflows.md) for the Plan/Run/Task spec.
