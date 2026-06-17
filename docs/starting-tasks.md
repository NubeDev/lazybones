# Starting tasks: the "I can't start a task" problem

> Status: short-term fixes landed; long-term plan proposed (not yet built).
> Audience: anyone working on the board UI, the API, or the `hcom` run loop.

## TL;DR

"Starting a task" in lazybones is **not one action** — it's a handoff between two
actors:

- **You (the UI)** = the *control plane*. You author tasks and **promote** them to
  `ready`. That is the only "go" signal a human gives.
- **The `hcom` run loop** = the *execution plane*. It watches for `ready` tasks,
  provisions a git worktree, calls `POST /tasks/:id/claim` (`ready → running`),
  and runs the agent.

A task only runs when **both** happen: you promote it **and** a loop is alive to
claim it. The UI cannot (and must not) mark a task `running` itself — doing so
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

1. **The loop is external and doesn't honour `worktree_mode` yet.** The run loop
   is `hcom` (`hcom run lazybones`), **not a crate in this workspace**. The UI now
   *captures and stores* the worktree intent and it appears in the task JSON, but
   `hcom`'s claim logic must be updated to *read* it. Until then, `reuse`/`branch`
   are recorded but not obeyed — the loop still does its current worktree add.

2. **A `ready` task with no loop running just sits there, unexplained.** Nothing
   tells the operator "promoted successfully, but nothing is consuming the queue."

## Proposed long-term fix

The goal: make the two-actor model **legible and honest** so "what will start it"
is never a mystery, and make worktree intent a **real contract** the loop honours.

### A. Make the contract explicit in the data model (done) and in `hcom` (todo)

`worktree_mode` is the durable contract. `hcom` should, on claim:

- `new` → `git worktree add` a fresh branch (current behaviour).
- `reuse` → use the existing `task.worktree` path; error clearly if it's missing.
- `branch` → run in the main checkout on `task.branch`; create no worktree.

This belongs in the `hcom` repo. The API already exposes everything it needs
(`worktree_mode`, `worktree`, `branch` on the task). No further API change should
be required — keep the contract in the task document, not in the claim body.

### B. Surface run-loop liveness in the UI

Add a lightweight "is a loop connected?" signal so a promoted task is never a
silent dead end. Options, cheapest first:

- **Heuristic banner (no backend change):** if any task is `ready` (or `running`)
  but none has a recent heartbeat / nothing has been claimed, show
  "No run loop connected — ready tasks won't execute until you start `hcom`."
- **Explicit liveness (small backend change):** have `hcom` register/heartbeat a
  loop presence the API exposes (e.g. `GET /engine` or a `loop_seen_at`), and the
  UI shows a green/grey "loop: connected / idle" pill in the toolbar.

Recommendation: ship the heuristic banner now; add explicit liveness when the
`hcom` change in (A) lands, since both touch the same claim path.

### C. Keep the human action honest

Never let the UI call `/claim`. "Start" = promote to `ready`, always. The loop
owns `ready → running`. This avoids zombie `running` tasks and keeps a single
source of truth for "is this actually executing."

### D. Documentation

This doc + a short "lifecycle" section in the UI README so the two-actor model is
discoverable from the code, not just tribal knowledge:

```
pending ──promote (you)──▶ ready ──claim (hcom loop)──▶ running ──▶ gating ──▶ done
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
