# SCOPE — GitHub issue linkage for tasks

Link a task to a GitHub issue: create one from the task's title/spec, or attach an
existing one; close it automatically when the task reaches `done`; and pull the
issue's state back so closing/reopening the issue on GitHub drives the task. This
is a **backend-first** delivery — the UI surface is deferred.

## Goal

A task can carry a pointer to one GitHub issue plus a small close-policy flag. Three
operator actions (create / link / unlink) and two automatic behaviours
(close-on-done, and the reverse issue→task sync) wire the task lifecycle to the
issue. The issue lives on GitHub; the task stores only a pointer and last-known
state — never a mirror of labels/assignees/body.

## What we reuse (no new GitHub plumbing)

`lazybones-gh::Gh` already wraps the `gh`/`git` binaries and exposes every primitive
needed: `issue_create(dir, title, body) -> url`, `issue_view(dir, n) -> Issue`,
`issue_close(dir, n)`, `issues(dir, state)`, and `ensure_auth()`. The `gh`-CLI vs
`octocrab` decision is therefore already settled by the codebase — **extend the
existing crate, add no new HTTP client.**

**Repo targeting comes for free.** `Workspace.repo` is the absolute filesystem path
to the checkout, and every `Gh` issue method takes a `dir`. Running `gh` *inside*
that path lets it infer `owner/repo` from the git remote — no slug parsing. A task's
repo is resolved by `task.run_id → Run.workspace.repo`. A **standalone task
(`run_id == None`) has no repo**, so issue actions are rejected with a clear error.

## Data model — 3 fields on `Task`

In [crates/lazybones-store/src/task/model.rs](crates/lazybones-store/src/task/model.rs),
all `#[serde(default)]` so existing rows stay readable:

- `issue_url: Option<String>` — the linked issue URL (covers both "created by us"
  and "linked existing"; provenance is irrelevant once linked). `None` = unlinked.
- `issue_close_on_done: bool` (default `false`) — close the issue on the `done`
  transition.
- `issue_synced_state: Option<IssueSyncState>` — last-known issue state
  (`Open`/`Closed`), so the reverse poll detects a *change* instead of re-acting
  every tick. `None` until first sync.

A small `IssueSyncState { Open, Closed }` enum (lowercase wire form, `parse`/`as_str`
mirroring `WorktreeMode`'s pattern). A free helper to extract the issue **number**
from a stored URL (the `gh` methods key off the number).

Touch points for the new fields (each is a mechanical add):
- `Task::seed(..)` — initialise all three to default.
- task row mapping ([crates/lazybones-store/src/task/row.rs](crates/lazybones-store/src/task/row.rs)) — persist + read back.
- `upsert` / `update` paths so authoring + edits round-trip the fields.
- API DTO ([crates/lazybones-api/src/dto.rs](crates/lazybones-api/src/dto.rs)) + TS mirror ([ui/src/types/task.ts](ui/src/types/task.ts)) — fields exposed read-only for now (UI write surface deferred).

## Behaviours

### 1. Create / link / unlink (operator actions)

New engine-layer module `scheduler::issue` (or a `lazybones-engine` helper) holding
the three actions; each resolves the repo dir from the task's run, then calls the
matching `Gh` method:

- **create** — `issue_create(repo_dir, &task.title, &task.spec)` → store returned
  URL into `issue_url`, set `issue_synced_state = Open`.
- **link** — given a URL or `#number`, `issue_view(repo_dir, n)` to validate it
  resolves → store `issue_url` + current state into `issue_synced_state`.
- **unlink** — clear all three fields. Does **not** touch the GitHub issue.

Exposed via new API routes (thin handlers next to the existing task routes) so a
future UI calls them; the UI never shells out to `gh` itself. `ensure_auth()` is
checked up front and a missing/unauthed `gh` returns a clear, surfaced error rather
than failing silently.

### 2. Close-on-done (task → issue)

Hook the **`Done`** path in
[crates/lazybones-engine/src/scheduler/finish.rs](crates/lazybones-engine/src/scheduler/finish.rs)
(`gate_and_land`, after the task is committed to `done`). If `issue_url.is_some() &&
issue_close_on_done`: resolve the number, `issue_close(repo_dir, n)`, set
`issue_synced_state = Closed`. **Best-effort** — a failed close logs a warning and
never blocks or reverts the task (matches the module's "never wedge the loop"
contract). This lives in the engine, not the store, because the store layer must not
shell out — and it covers auto-completed tasks too, not just manual ones.

### 3. Reverse sync (issue → task) — the one new mechanism

Piggyback the existing scheduler tick
([crates/lazybones-engine/src/scheduler/tick.rs](crates/lazybones-engine/src/scheduler/tick.rs)),
as a new best-effort step after `hcom_tail` (so it never blocks claim/spawn):

- For every task with `issue_url.is_some()`, `issue_view(repo_dir, n)` and compare
  the live `state` against `issue_synced_state`.
- **Issue closed on GitHub** (`synced != Closed` && live == closed) **and the task
  is not already `done`** → transition the task to `done` (the issue is the source
  of truth for "this work is no longer needed"). Then update `issue_synced_state`.
- **Issue reopened** (`synced == Closed` && live == open) → **revive** the task if
  it is in a terminal/blocked state (mirrors the manual `Revive` transition);
  otherwise just record the state.
- No diff → nothing to do.

Notes:
- Poll, not webhook — reuses the loop we already have, needs zero new infra. A
  coarse cadence (every Nth tick, configurable) keeps the extra `gh` calls cheap;
  webhooks can later slot in behind the same state-diff logic.
- The `done` transition we drive here takes a `commit` — the reverse-sync close has
  no agent commit, so this step needs a way to land a task `done` *without* a fresh
  commit (reuse the existing commit if present, or extend the transition to allow a
  commit-less external completion). **Flagged as the one design wrinkle** to settle
  in implementation.
- Loop-safety: close-on-done sets `synced = Closed`, and the reverse step skips tasks
  already `done`, so a close we initiate does not bounce back as an external event.

## Out of scope (v1)

- UI for create/link/unlink and the close-on-done toggle (backend + API only now).
- Webhook ingestion (poll only).
- Syncing issue body/labels/assignees/comments either direction.
- Multiple issues per task (exactly one pointer).
- Per-task repo override (repo always comes from the workflow).

## Files touched

- `crates/lazybones-store/src/task/model.rs` — 3 fields + `IssueSyncState` + url→number helper.
- `crates/lazybones-store/src/task/row.rs`, `upsert.rs`, `update.rs` — persist/round-trip.
- `crates/lazybones-engine/src/scheduler/` — new `issue.rs` (actions + sync), wire into `finish.rs` + `tick.rs`.
- `crates/lazybones-engine/src/config.rs` — reverse-sync cadence knob.
- `crates/lazybones-api/src/dto.rs` + new routes — expose fields + create/link/unlink.
- `ui/src/types/task.ts` — mirror the new fields (read-only).

## Acceptance

- A task in a workflow can create a GitHub issue from its title/spec; the URL is stored.
- An existing issue can be linked by URL/`#number` and validated.
- A task with `issue_close_on_done` reaching `done` closes its issue (best-effort, non-blocking).
- Closing the linked issue on GitHub drives the task to `done` within a few ticks; reopening revives it.
- Standalone (run-less) tasks reject issue actions with a clear error.
- Existing task rows without the new fields load unchanged.
