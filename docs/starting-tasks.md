# Starting tasks: the "I can't start a task" problem

> Status: short-term fixes landed; long-term plan proposed (not yet built).
> Audience: anyone working on the board UI, the API, or the scheduler.

## TL;DR

"Starting a task" in lazybones is **not one action** — it's a handoff between two
actors:

- **You (the UI)** = the *control plane*. You author tasks and **promote** them to
  `ready`. That is the only "go" signal a human gives.
- **The scheduler** (a Rust task inside `lazybonesd`) = the *execution plane*. It
  watches for `ready` tasks, provisions a git worktree, claims them
  (`ready → running`), and spawns the agent via the hcom CLI.

A task only runs when **both** happen: you promote it **and** the daemon (hence the
scheduler) is up. The UI cannot (and must not) mark a task `running` itself — doing so
would create a `running` task with no agent behind it (a "zombie").

This split is correct by design, but it was **invisible**, so it read as broken.

## Symptoms reported

1. "Drag-and-drop doesn't work — I can drag a card but can't drop it."
2. "I can't see how to start a task."
3. "It's in Ready and it's the first task — what will start it? Seems like broken
   logic."

## Root causes (what was actually wrong)

These are distinct bugs that compounded into one confusing experience.

### 1. HTML5 drag-and-drop drop was gated twice

A drop only fires if `dragover` **both** calls `preventDefault()` **and** leaves a
non-`"none"` `dropEffect`. An early fix made `preventDefault()` unconditional but
then re-gated the drop via `dataTransfer.dropEffect = isTarget ? "move" : "none"`.
When `isTarget` was false (e.g. a mid-drag refetch dropped the dragged task from
the freshly-rebuilt `byId` map), `dropEffect = "none"` silently rejected the drop.
**Fix:** always set `dropEffect = "move"`; let `dropAction()` be the sole arbiter
of legality. (`ui/src/features/tasks/board-column.tsx`)

### 2. Dependency rules made most cards un-droppable — silently

The seed tasks form a chain: `store → auth → api → cli`. The backend only allows
`pending → ready` when **every dependency is `done`**, so only `store` (no deps)
was promotable. Dropping `api`/`auth`/`cli` on Ready was a correct **no-op** — but
with **zero feedback**, so it looked like the board was broken.
**Fix:** dim cards that can't be promoted and show a tooltip naming the unmet deps
(`promoteBlockedReason()` in `ui/src/features/tasks/drag-rules.ts`,
`ui/src/features/tasks/task-card.tsx`). Caveat found while verifying: the
`animate-fade-up` entrance animation uses `animation-fill-mode: both`, which pins
`opacity: 1` and overrode the dimming class — the card now skips the animation
when blocked so `opacity-50` takes effect.

### 3. There was no Start button at all

The only board actions were New task, Promote ready, Edit, Block, Delete. Nothing
said "start this one." **Fix:** a Start dialog on pending tasks that promotes to
`ready` (and lets you pick a worktree mode first). See "Worktree mode" below.

### 4. `PATCH` was missing from the CORS allow-list (the real blocker)

`crates/lazybones-api/src/cors.rs` allowed `GET,POST,PUT,DELETE,OPTIONS` but **not
`PATCH`**, even though `PATCH /tasks/:id` is a real route. Every cross-origin
`PATCH` from the browser failed its CORS preflight and surfaced as a misleading
`ApiError(0, "Cannot reach lazybonesd")`. `curl` was unaffected (it skips CORS),
which is exactly why this hid — and why **Edit task** was also silently broken
from the browser. **Fix:** add `Method::PATCH` to the allow-list.

> Lesson: this was only found by driving a real headless browser (Playwright) and
> diffing browser behaviour against `curl`. Contract-level reasoning alone missed
> it three times. **For DnD / CORS / preflight issues, observe a real browser.**

## What shipped (short-term)

- DnD drop always reaches the handler; legality decided in one place.
- Blocked cards are dimmed with a "waiting on X, Y" tooltip.
- A **Start** button on pending tasks → dialog that (optionally) sets the worktree
  mode, then promotes to `ready`. It never fakes a claim.
- A per-task **worktree mode** (`new` | `reuse` | `branch`) carried end-to-end:
  store → API DTO → UI. `new` = isolated `git worktree add` (today's behaviour);
  `reuse` = an existing worktree path; `branch` = the main checkout on a branch,
  no new worktree. `#[serde(default)]` keeps pre-existing tasks readable.
- `PATCH` allowed in CORS.

## The remaining gap (why this needs a long-term fix)

Two things are still only half-solved:

1. **No loop exists yet to honour `worktree_mode`.** The scheduler is planned as a
   Rust task inside `lazybonesd` (`src/scheduler/`, see [vision.md](vision.md)) —
   **not** an external `hcom run` script (that earlier framing is superseded). The
   UI already *captures and stores* the worktree intent (it appears in the task
   JSON); the scheduler must *read* it on claim. Until the scheduler is built,
   `reuse`/`branch` are recorded but not obeyed.

2. **A `ready` task with nothing consuming it.** Today there is no execution plane
   at all. Once the scheduler lives in the daemon this dissolves: if `lazybonesd` is
   up, the queue is being drained — there is no separate loop to be "disconnected."

## Proposed long-term fix

The goal: make the two-actor model **legible and honest** so "what will start it"
is never a mystery, and make worktree intent a **real contract** the loop honours.

### A. Make the contract explicit in the data model (done) and in the scheduler (todo)

`worktree_mode` is the durable contract. The Rust scheduler should, on claim:

- `new` → `git worktree add` a fresh branch (current intended behaviour).
- `reuse` → use the existing `task.worktree` path; block clearly if it's missing.
- `branch` → run in the main checkout on `task.branch`; create no worktree.

This is **lazybones' own code** (`src/scheduler/`), not the hcom repo — the
scheduler provisions the worktree *before* it shells out to `hcom` to spawn the
agent. The task document already carries everything it needs (`worktree_mode`,
`worktree`, `branch`); keep the contract there, not in any claim body.

### B. Loop liveness — mostly dissolved by the in-process scheduler

Once the scheduler lives inside `lazybonesd`, "is a loop connected?" collapses into
"is the daemon up?" — which the UI already knows from `GET /health`. There is no
separate process to be disconnected. The only residual signal worth showing is
whether the **hcom CLI** is installed/usable (`GET /engine`), since the scheduler
shells out to it; surface that as a grey/green "hcom available" pill.

### C. Keep the human action honest

Never let the UI call `/claim`. "Start" = promote to `ready`, always. The loop
owns `ready → running`. This avoids zombie `running` tasks and keeps a single
source of truth for "is this actually executing."

### D. Documentation

This doc + a short "lifecycle" section in the UI README so the two-actor model is
discoverable from the code, not just tribal knowledge:

```
pending ──promote (you)──▶ ready ──claim (scheduler)──▶ running ──▶ gating ──▶ done
                                                                              └─ unlocks dependents
```

## Verification notes (for whoever picks this up)

- Reproduce DnD/CORS issues in a **real browser** (Playwright against the Vite dev
  server on `:1420` + `lazybonesd serve` on `:7878`). `curl` will mislead you on
  anything CORS- or preflight-related.
- The dev loop token is `lazybones-loop` (`LAZYBONES_LOOP_TOKEN`); the UI stores it
  in `localStorage["lazybones-loop-token"]` for guarded mutations
  (`promote`/`ready` need `Claim`; `create`/`update` need `Author`).
- End-to-end check that passed: authoring a dep-free task, opening it, picking
  "Same branch", and clicking Start produced `PATCH /tasks/:id` (mode) then
  `POST /tasks/:id/ready`, leaving the task `ready` with `worktree_mode = branch`.

## Open questions

- Should `hcom` reject a `reuse`/`branch` claim it can't satisfy (missing path /
  dirty branch) by **blocking** the task with a reason, so it shows up in the UI?
- Do we want a worktree mode default at the **run** level (config) as well as
  per-task, for users who always want `branch`?
- Is an explicit loop-liveness endpoint worth it, or is the heartbeat heuristic
  enough for a single-user local tool?
```
