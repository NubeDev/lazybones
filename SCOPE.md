# SCOPE — lazybones: dead-simple multi-agent build orchestration on hcom

Scope for a small orchestrator that builds software by running many AI coding
sessions in parallel and gating each one on a real green build. The orchestration
*engine* is **hcom** — its workflow scripts already spawn, isolate, watch, and
clean up agents across terminals, so lazybones does **not** reimplement a loop.
The durable *brain* is a small Rust binary exposing a **REST API over embedded
SurrealDB**, and SurrealDB is used for everything it is good at: documents (tasks,
runs), a **graph** (dependency + memory edges), **vectors** (AI memory the agents
recall), and **live queries** (realtime status). The queue, specs, status, run
history, and agent memory all live in the database — it is the single source of
truth, so a run survives a restart and can be inspected or driven over HTTP. YAML
(`lazybones.yaml`, `workfile.yaml`) is only a **seed format**: you author in it,
import once, and the DB is authoritative from then on. Each unit of work is a
**task**, not a `WS-01`.

This replaces the prior `docs/sessions/` system (sequential, single-branch,
`STATUS.md` + `WS-xx.md` + a cron `claude -p` loop whose log filled with hundreds
of identical "idle" lines). lazybones is parallel by default, worktree-isolated,
durable in a database, and has no bespoke loop algorithm to babysit.

## Principles

1. **hcom is the engine; lazybones is the queue + the gate.** The loop is an hcom
   workflow script (`hcom run lazybones`). lazybones owns only what hcom doesn't:
   the durable queue, the commit/push gate, worktree lifecycle, and the REST/state
   surface. No cron loop, no hand-rolled spawn algorithm.
2. **Parallel by default; the worktree is how it stays safe.** Every running task
   gets its own `git worktree` + branch, so N agents edit N trees with zero
   collisions. Worktree isolation is not an add-on — it is the parallelism
   mechanism. (It can be turned off, which forces tasks to serialize.)
3. **Restartable because state is in the database, not in a process.** Task status,
   run history, and heartbeats live in SurrealDB behind the REST API. Kill the
   script, the machine, the agents — on restart the loop asks the API what is
   `ready`/`running`/`done`, reconciles against `git worktree list` and `hcom list`,
   and resumes. No hidden state, no markdown to parse.
4. **A task is done only when it committed AND pushed AND the gate is green.**
   `done` is earned: the agent commits + pushes its branch, the orchestrator
   re-runs the gate (`cargo test` / `clippy`, configurable) in the worktree, and
   only a green re-run flips the task to `done` in the database. A red gate is
   `blocked`, never silently `done`.
5. **Generic, configured per-repo.** Target repo, base branch, gate commands,
   remote, and concurrency come from `lazybones.yaml`. lazybones lives in this repo
   but is not hardwired to rubix.
6. **The database is the source of truth; YAML is a seed.** `lazybones.yaml` and
   `workfile.yaml` are imported into SurrealDB once (`POST /workfile/sync`,
   idempotent upsert) and the DB is authoritative after — queue, task specs,
   config, status, history, and memory are all DB records read/written over REST,
   never a markdown log or a re-parsed file mid-run.
7. **Use SurrealDB for what it is good at, including AI memory.** Documents for
   tasks and run events; a **graph** for dependency edges and memory relations;
   **vectors** so an agent starting a task *recalls* the relevant past decisions,
   gotchas, and follow-ups instead of re-reading prose charters; **live queries**
   so status is a push feed, not a poll. The old `WS-xx.md` "assumptions /
   deviations / follow-ups" prose becomes queryable, vector-indexed memory.
8. **One verb per file, ≤400 lines.** lazybones obeys
   [docs/FILE-LAYOUT.md](../docs/FILE-LAYOUT.md): verb-per-file route/command
   folders, names that are concepts, `mod.rs` is a barrel only.

## How it runs (one run, start to finish)

```
   lazybones.yaml ── config ──┐
   workfile.yaml  ── queue ───┤  hcom run lazybones "build the queue"
                              ▼
                ┌──────────────────────────────┐      REST / HTTP (JSON)
   the loop ───►│   lazybonesd  (Rust binary)   │◄──────────── you · scripts · agents
  (hcom script) │   axum REST  +  SurrealDB     │   GET /tasks   POST /tasks/:id/done
                │   embedded, file-backed       │   POST /tasks/:id/heartbeat   ...
                └───────────────┬──────────────-┘
                                │ durable queue + status + run log (restartable)
                                ▼
        ┌──────── hcom workflow script = the loop ────────┐
        │ 1. sync workfile.yaml → lazybonesd               │
        │ 2. ask /tasks?status=ready (deps met, owns free) │
        │ 3. for each, up to `concurrency`:                │
        │      git worktree add + branch  →  spawn 1 agent │
        │ 4. wait on hcom thread DONE (events --wait, no    │
        │    sleep); agents POST heartbeats to lazybonesd  │
        │ 5. gate: re-run tests in the worktree            │
        │ 6. green → push branch, merge, mark /done         │
        │    red   → mark /block, keep worktree for triage │
        │ 7. loop until no ready/running tasks remain       │
        └───┬──────────────┬──────────────┬───────────────┘
            ▼              ▼              ▼
        agent:auth     agent:store     agent:api     ← headless claude/codex/…, parallel
        wt/auth        wt/store        wt/api        ← isolated git worktrees
        lazy/auth      lazy/store      lazy/api      ← branches: commit + push each
```

The loop never writes feature code. It reads the queue, spawns agents, runs the
gate, advances status. Every line of feature work happens inside an agent, in a
worktree, on its own branch.

## Components

### `lazybonesd` — the brain (Rust binary, REST + SurrealDB)

A small axum server over an **embedded, file-backed SurrealDB** (same engine choice
as the rubix store: `kv-surrealkv`). It is the single source of truth for a run, and
it leans on each SurrealDB capability rather than treating the DB as a flat KV:

- **Documents** — task records (the full spec text lives here, seeded from the
  workfile, *not* re-read from disk at runtime), run records, config records.
- **Graph** — `task ->depends_on-> task` edges drive readiness; `task ->learned->
  memory` and `memory ->relates_to-> memory` edges link a task to what it produced
  and to neighbouring knowledge.
- **Vectors** — memory records carry an embedding so an agent can semantically
  recall the right past decision (see **AI memory** below).
- **Live queries** — status changes are pushed (SSE/WS) to a dashboard or to the
  loop, so nobody polls and there is no idle-log spam.
- **Run log as rows, not prose.** Every transition is a queryable event row
  (task, from→to, actor, correlation id, timestamp) — the structured replacement
  for the old appended loop log.
- **Heartbeats.** Running agents POST liveness; a task `running` with a stale
  heartbeat and no live worktree/agent is reclaimable on the next loop pass.
- **REST surface** (verb-per-file under `src/api/`):

  | Method · path | Job |
  | --- | --- |
  | `GET /health` | liveness |
  | `POST /workfile/sync` | import `workfile.yaml` + `lazybones.yaml` into the DB (idempotent upsert); DB authoritative after |
  | `GET /tasks` | list + filter by status (`?status=ready`) |
  | `GET /tasks/:id` | one task: spec text, status, deps, history |
  | `POST /tasks/:id/claim` | `ready → running`, records session + worktree + branch |
  | `POST /tasks/:id/heartbeat` | liveness ping from the agent |
  | `POST /tasks/:id/done` | `gating → done`, records commit sha + push ref |
  | `POST /tasks/:id/block` | `* → blocked` with a reason |
  | `GET /runs/:id` | full event history for a run |
  | `GET /stream` | live-query feed of status changes (SSE) — for dashboards + the loop |
  | `POST /memory` | agent writes a memory (decision/gotcha/follow-up); server embeds it |
  | `GET /memory/recall` | vector + graph recall for a task (`?task=auth&q=…&k=8`) |

### AI memory (SurrealDB vectors + graph) — agents recall, they don't re-read

The load-bearing new idea. The valuable residue of every task — the assumptions it
made, the deviations it took, the gotchas it hit, the follow-ups it left (exactly
the `WS-xx.md` "assumptions / deviations / follow-ups" sections) — is captured as
**memory records**, not buried in prose:

- **Write.** On finishing or blocking, the agent `POST /memory` with short, atomic
  notes (one fact each). `lazybonesd` embeds the text and stores it with a vector,
  a kind (`decision | gotcha | deviation | follow-up`), and a `task ->learned->
  memory` edge.
- **Recall.** On starting a task, the agent (and the loop) `GET /memory/recall` —
  vector similarity against the task's spec/goal, widened along the graph to the
  memories its dependency tasks produced. The agent opens its session already
  knowing "in `store`, SurrealDB 3.x typed verbs need `SurrealValue`, not serde" —
  it does not rediscover it.
- **Why it matters.** This is the difference between N isolated sessions and a team
  with shared, growing memory. It directly attacks the failure mode of the old
  system (each agent re-reading a fixed `READ FIRST` list and re-learning the same
  lessons). Memory is per-run by default and can be promoted to a durable,
  cross-run store.

### `lazybones.sh` — the loop (hcom workflow script)

Installed to `~/.hcom/scripts/lazybones.sh`, run with `hcom run lazybones "<goal>"`.
Follows the hcom authoring rules exactly: `--thread` on every send/wait (isolation),
`--go --headless` on every launch, `--tag` per task for group routing,
`hcom events --wait` instead of `sleep`, `trap cleanup ERR INT TERM` to never orphan
a headless agent, and it parses `--name` to forward identity. Per ready task it:

1. `git worktree add <root>/wt/<id> -b lazy/<id> <base_branch>` (unless worktrees off),
2. `POST /tasks/:id/claim`,
3. spawns one headless agent (`hcom 1 <tool> --tag <id> --go --headless --hcom-prompt …`)
   pointed at the task spec + the agent charter, told to implement, commit, push,
   then signal `DONE` on the thread,
4. waits on the DONE event, runs the gate in the worktree, advances the task in
   `lazybonesd`, and on success merges `lazy/<id>` back and removes the worktree.

### `lazybones.yaml` — config seed (env-overridable)

Target repo, base branch, push remote, gate commands, concurrency, worktree toggle,
agent tool, and the `lazybonesd` address. Imported into the DB on `sync` and
authoritative there after; only the DB location + bind are true boot config. Every
key is overridable by an environment variable (`LAZYBONES_*`) for headless/CI use.
See the committed example.

### `workfile.yaml` — the queue seed ("the hcom workfile")

The ordered list of tasks: each has a friendly `id` (e.g. `auth`, not `WS-01`), a
title, the spec (inline text or a path to a `tasks/*.md` to import), dependency ids,
optional `owns` globs (a second safety net so two tasks never claim overlapping
paths), and an optional per-task agent tool. On `sync` it is upserted into SurrealDB
as task documents + `depends_on` graph edges; **the DB is the queue after that** —
you can edit tasks over REST, and re-importing reconciles. See the committed example.

### `tasks/<id>.md` — a spec seed per task

Optional human-written spec (goal, deliverables, done-definition, tests), named by
concept (`tasks/auth.md`), not `WS-01.md`. It is **imported into the task record**
on `sync`; agents read the spec from the DB (`GET /tasks/:id`), not from disk. Edit
the file and re-sync, or edit the record directly.

## Proposed file layout (verb-per-file, ≤400 lines)

```
lazybones/
  SCOPE.md                       # this doc
  lazybones.yaml                 # config (env-overridable)
  workfile.yaml                  # the task queue
  Cargo.toml                     # the lazybonesd crate
  tasks/
    <id>.md                      # one spec per task
  scripts/
    lazybones.sh                 # the hcom workflow script (installed to ~/.hcom/scripts/)
    install-hcom-script.sh       # symlink/copy lazybones.sh into ~/.hcom/scripts/
  src/
    main.rs                      # parse config, open store, serve
    configure.rs                 # load lazybones.yaml + LAZYBONES_* env overrides
    state.rs                     # AppState { store handle, config }
    workfile/
      mod.rs                     # barrel
      parse.rs                   # workfile.yaml → task records
      sync.rs                    # idempotent upsert into the store
    store/
      mod.rs                     # barrel
      connect.rs                 # open embedded SurrealDB (kv-surrealkv)
      task.rs                    # task record + status transitions
      depend.rs                  # depends_on graph edges + readiness query
      event.rs                   # run-log event rows
      live.rs                    # live-query subscription → SSE feed
    memory/
      mod.rs                     # barrel
      embed.rs                   # text → embedding vector
      write.rs                   # store a memory + learned/relates_to edges
      recall.rs                  # vector + graph recall for a task
    api/
      mod.rs                     # Router wiring only
      health.rs                  # GET /health
      list.rs                    # GET /tasks
      get.rs                     # GET /tasks/:id
      claim.rs                   # POST /tasks/:id/claim
      heartbeat.rs               # POST /tasks/:id/heartbeat
      done.rs                    # POST /tasks/:id/done
      block.rs                   # POST /tasks/:id/block
      runs.rs                    # GET /runs/:id
      stream.rs                  # GET /stream (SSE live status)
      memorize.rs                # POST /memory
      recall.rs                  # GET /memory/recall
    error.rs                     # lazybones error enum (thiserror)
  tests/
    workfile_parse_test.rs
    task_transition_test.rs
    api_claim_test.rs
    api_done_gate_test.rs
```

## Task lifecycle (the state machine)

```
  pending ──deps met & owns free──► ready ──claim──► running ──agent DONE──► gating
                                                        │                       │
                                              heartbeat stale + no              │ gate green
                                              worktree → reclaim ──┐            ▼
                                                                   └──► ready   done  (commit+push recorded)
  any state ──unrecoverable──► blocked (reason recorded; worktree kept for triage)
```

`done` requires all three: a commit, a successful push, and a green gate re-run by
the orchestrator. Nothing else flips a task to `done`.

## Restart & recovery

There is no in-memory run state. On every loop entry the script reconstructs
reality from three durable sources and reconciles them:

- `GET /tasks` — what lazybonesd believes about each task,
- `git worktree list` — which task trees actually exist,
- `hcom list` — which agents are actually alive.

A `running` task with no live agent and a stale heartbeat is reclaimed to `ready`
(its worktree is reused — agent work is idempotent: it reads the task + git to see
what already landed). A `gating` task is re-gated. Safe to kill and resume at any
point.

## Non-goals

- **No bespoke loop algorithm.** hcom's workflow runtime is the loop; lazybones
  adds a queue and a gate, nothing more.
- **No file as the runtime source of truth.** Status, queue, specs, and memory are
  SurrealDB over REST. YAML/markdown are seeds imported once; there is no
  `STATUS.md`, no appended loop log, no per-task front-matter to keep in sync, and
  nothing re-parsed from disk mid-run.
- **No prose-only knowledge.** Decisions/gotchas/follow-ups are vector-indexed
  memory records, recalled on demand — not buried in a `WS-xx.md` to be re-read.
- **No `WS-01`-style ids.** Tasks have friendly concept ids.
- **No separate database.** Embedded SurrealDB, file-backed, single binary — same
  engine the rubix platform already standardizes on.
- **No sequential-only mode as the default.** Parallel-with-worktrees is the
  default; serial is the degraded fallback when worktrees are disabled.
- **No agent-tool lock-in.** Any hcom-supported tool (claude, codex, gemini, …)
  can run a task; chosen per-run or per-task.

## Open questions

1. **Merge strategy on gate-pass.** Fast-forward/merge `lazy/<id>` into base
   automatically, or open a PR and let a human (or a reviewer agent) merge? Default
   proposal: auto fast-forward when the gate is green and base hasn't moved under
   it; fall back to PR on conflict.
2. **Dependency vs. ownership.** Both `deps` (ordering) and `owns` (collision
   guard) exist. With worktree isolation, `owns` matters only at merge time — do we
   need it at all, or only when worktrees are disabled?
3. **Gate location of truth.** Agent commits+pushes and the orchestrator re-gates —
   is one canonical gate run (orchestrator-only) enough, or should the agent also
   self-gate before signalling DONE to fail faster?
4. **Push target for parallel branches.** One remote with N `lazy/*` branches, or a
   stacked/queued merge to avoid base churn when many tasks finish at once.
5. **Where `lazybonesd` runs.** Spawned by the workflow script for the run's
   lifetime, or a long-lived service so the REST API and history outlast a single
   run (better for inspection, more to manage).
6. **Cross-tool tasks.** A task that needs design-then-implement across two tools
   (hcom pattern 5) — modeled as one task with an internal duo, or two dependent
   tasks?
7. **Embedding provider for memory.** Where do memory vectors come from — a local
   model (offline, no key, lower quality), an API embedder (better recall, needs a
   key + network), or pluggable? And the dimension/index choice in SurrealDB.
8. **Memory scope + lifetime.** Per-run only, or a durable cross-run store that
   accumulates project knowledge across builds? If cross-run, how is stale or
   wrong memory retired (it reflects what was true when written)?
```
