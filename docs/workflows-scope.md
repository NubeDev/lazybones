# Workflows, Task Templates, and inherited git config

> Status: spec, ready to build. Backend-first.
> Audience: whoever implements the Workflow/Template layer.
> Read [vision.md](vision.md) and [starting-workflows.md](starting-workflows.md)
> first for the Plan/Run/Task framing and the layering rationale. This doc is the
> **concrete, simplified v1** of that vision: one-off workflows + reusable task
> templates, with git config that flows workflow → task.

## The user story this exists to serve

> 1. Open lazybones. Add reusable **task templates** (e.g. `code-review`,
>    `open-pr`) once, globally.
> 2. Make **workflow-1 on repo-abc**: set a workspace (repo + git mode, e.g. a new
>    worktree). Add a task `new-api` that uses the `open-pr` template. The task
>    inherits the workspace's git mode.
> 3. Make **workflow-2 on repo-abc**, but with **reuse worktree** pointing at
>    workflow-1's task. Add a UI task for the new API.
> 4. Make **workflow-3** on a different repo with different tasks.
> 5. **All three run at once with no collisions.**

Everything below is the minimum set of nouns and rules to make exactly that work.

## Decision: v1 is one-off workflows + reusable templates

A **Workflow** in v1 *is a Run* — a concrete, one-off thing you build on a repo and
execute. The only reusable noun is the **Template** (a task recipe).

We deliberately **do not** build the reusable **Plan** layer yet (a saved workflow
recipe you instantiate repeatedly). But every new type must leave a clear seam for
it — add a doc-comment on each model noting "a future `Plan` would be an ordered
set of templates instantiated as a whole; this type is the half a Plan reuses."
See [starting-workflows.md](starting-workflows.md) for the full Plan spec we are
deferring.

This keeps v1 to **three nouns**: Template, Workflow (= Run), Task.

## The nouns

### `template` — a reusable task recipe (global, stateless)

No lifecycle, no run, no claim state. Picked from a list when adding a task to a
workflow; instantiating it produces a concrete `Task`.

```text
id: string                      # friendly, unique install-wide (e.g. "open-pr")
title: string
description: string             # optional, shown in the picker
spec_template: string           # starting spec text for tasks made from it
default_tool?: string           # inherited by the task unless overridden
default_worktree_mode?: new | reuse | branch   # rarely set; usually inherit the workspace
created_at, updated_at
```

`default_worktree_mode` exists but should almost always be `None` — git mode is
normally a property of the *workspace*, not the recipe. It is here only for the
rare template that is intrinsically tied to a mode.

### `run` — a Workflow (one concrete, one-off run)

Bound to a **workspace**: the repo + the git config its tasks inherit.

```text
id: string                      # friendly, unique install-wide (e.g. "workflow-1")
title: string
workspace:
  repo: string                  # absolute path to the target git repo
  base_branch?: string          # default: global EngineConfig.base_branch
  branch_prefix?: string        # default: global EngineConfig.branch_prefix
  worktree_mode: new | reuse | branch   # the default git mode for this workflow's tasks
lifecycle: active | stopped     # human-set only; stopped is a reversible pause
created_at, started_at?
```

**Run *state* is derived, never stored** (avoids drift from the tasks beneath it).
Compute it from the run's lifecycle + tasks on read. Only `done` (derived) and a
hard `delete` are terminal — `stopped` is a reversible pause, so the UI never
lies that a run is finished:

```text
done              if every task is done            (wins: nothing left to resume)
stopped           if lifecycle = stopped           (paused; reversible via resume)
needs-attention   if any task is blocked
running           if any task is running or gating
ready             if any task is ready
draft             otherwise (no task promoted yet)
```

Only `lifecycle` is a stored, human-set field.

### `task` — unchanged executable unit, plus three link fields

```text
run_id?: string         # FK to the parent workflow run (None = standalone task)
template_id?: string    # provenance: which template it was instantiated from
reuse_from?: string     # for worktree_mode=reuse: the task id whose worktree to reuse
```

The dotted board label (`workflow-1 · new-api`) is **derived** from `run_id`, never
parsed back into truth (SCOPE.md principle 6 — relationships are the key).

## The one rule that makes the user story work: git-config inheritance

A task's effective git settings are resolved **at claim time, in the scheduler**,
per field, most-specific-wins:

```text
effective.repo          = run.workspace.repo            ?? global target_repo
effective.base_branch   = run.workspace.base_branch     ?? global base_branch
effective.branch_prefix = run.workspace.branch_prefix   ?? global branch_prefix
effective.worktree_mode = task.worktree_mode (if set)   ?? run.workspace.worktree_mode ?? New
```

Notes:
- The **repo lives on the workspace**, not the task — workflow-1 and workflow-2 are
  the *same* repo with *different* modes, so the repo can't be a per-task thing.
- `worktree_mode` is the one field a task may override (e.g. a workflow defaults to
  `new` but one task says `branch`). Because `WorktreeMode` has a non-`None`
  default (`New`), "task didn't set it" can't be distinguished from "task set New"
  by value alone — so v1 keeps it simple: **a task with a non-default mode
  overrides; otherwise it inherits the workspace.** If that proves too coarse, make
  the task field `Option<WorktreeMode>` in a follow-up (noted in the prompt).
- A **standalone task** (`run_id = None`) keeps today's behaviour exactly: it reads
  only the global `EngineConfig`. Nothing about the existing task path changes.

### `reuse_from` (cross-workflow worktree reuse)

When a task's effective mode is `reuse`:
- if `reuse_from` is set → resolve to *that task's* stored `worktree` path; **block
  the task with a clear reason if that task has no worktree** (not yet claimed /
  already torn down).
- else → fall back to the task's own `worktree` field (today's `reuse` behaviour).

This is what lets workflow-2 reuse workflow-1's tree.

## Concurrency: 3 workflows at once is already the model

The scheduler already enforces **one global concurrency budget across all tasks**
(not per-run), and worktree isolation is the safety mechanism. So N active
workflows just contribute their `ready` tasks to the same global queue. The only
addition v1 needs:

- **Per-run fairness** so one big workflow can't starve the others: when more
  tasks are `ready` than the budget allows, round-robin the claim across distinct
  `run_id`s rather than taking them in list order. (If this is more than ~30 lines,
  it's acceptable to ship FIFO in v1 and leave a `TODO(fairness)` — note which you
  did.)

Collisions between concurrent workflows are prevented by worktrees (different
trees) and, where they share a tree via `reuse`, by hcom's `owns`/collision guard —
no new mechanism required.

## API surface (additive; nothing existing changes meaning)

| Method · path | Job |
| --- | --- |
| `GET /templates` | list task templates |
| `POST /templates` | create a template (`409` on duplicate id) |
| `GET /templates/:id` | fetch one |
| `DELETE /templates/:id` | delete one (returns whether it existed) |
| `GET /workflows` | list workflows with derived state + task counts |
| `POST /workflows` | create a workflow (workspace + empty task set) |
| `GET /workflows/:id` | detail: workspace, derived state, generated task ids |
| `POST /workflows/:id/tasks` | add a task to the workflow, optionally `from_template` |
| `POST /workflows/:id/start` | activate: promote eligible root tasks to `ready` |
| `POST /workflows/:id/stop` | pause (lifecycle=stopped): `hcom kill tag:<id>` + reclaim running tasks to `ready` (work kept) |
| `POST /workflows/:id/stop-reset` | pause AND reset unfinished tasks to `pending` (throw in-flight progress away; done kept) |
| `POST /workflows/:id/resume` | un-pause (lifecycle=active) + reset blocked tasks to `pending`; scheduler picks back up |

A `stopped` run promotes/claims nothing (the scheduler's `newly_ready`/claim both
read the parent run's lifecycle), and the task-level revive verbs (`retry`,
`auto-retry`, chat-revive) refuse with `409` until the run is resumed — so a paused
workflow can never quietly keep running. Stop is reversible; `delete` is the only
archive/tombstone path.

Capabilities (reuse the existing `Capability` enum — do **not** invent new variants
unless one genuinely doesn't fit): template + workflow authoring use `Author`;
`start` uses `Claim`; `stop`/`stop-reset`/`resume` use `Block`. The existing task
routes (`/tasks/...`) are untouched.

> Naming: use `/workflows` (not `/runs`) for the user-facing noun, since the user
> says "workflow". Internally the store table can be `run` to match
> [starting-workflows.md](starting-workflows.md) — but keep the public path
> `/workflows`. The existing `GET /runs/:id` (event history) stays as-is.

## What stays out of v1 (explicit non-goals)

- **No reusable Plans** (saved recipes instantiated repeatedly). Templates are the
  only reusable noun. Leave seams in the models.
- **Two lifecycle states** — `active | stopped` (stopped is a reversible pause).
  No separate cancelled tombstone; `delete` is the archive path.
- **No branching/conditional tasks** — linear + fan-out deps only (already
  supported by the `depends_on` graph).
- **No workflow YAML** as an authoring path — the DB is the product (SCOPE.md
  principle 6). The existing `workfile.yaml` seed is untouched and orthogonal.
- **No UI** in this pass — backend only, proven over REST. UI is the next pass.

## Done = these checks

1. `cargo build --workspace` clean; `cargo clippy --workspace --all-targets -- -D
   warnings` clean; edition 2024, `unsafe_code = "forbid"` respected (no `unsafe`,
   including in tests — see the prompt's env-var caveat).
2. `cargo test --workspace` green, **including a new integration test** that, over
   the REST API:
   - creates two templates (`code-review`, `open-pr`);
   - creates **3 workflows**: wf-1 + wf-2 on the *same* tempdir git repo (wf-1
     `worktree_mode=new`, wf-2 `worktree_mode=reuse` with `reuse_from` pointing at a
     wf-1 task), wf-3 on a *different* tempdir repo;
   - adds tasks (some `from_template`) to each;
   - starts all three; with a **fake hcom on PATH** (the stub from the existing
     `tick_walk_test`), drives ticks and asserts every workflow's tasks reach
     `done`, that wf-2's reused-tree task resolves wf-1's worktree path, and that
     all three made progress concurrently (none blocked the others).
3. Unit tests: inheritance resolver (task ?? workspace ?? global, all branches);
   `reuse_from` missing-target blocks; derived run-state for each case.
4. Existing tests and the REST contract still pass unchanged.

## Build order (each layer testable before the next)

1. **Store**: `template` model+row+verbs+schema; `run` model+row+verbs+schema
   (derived-state computed in a verb, not stored); add `run_id`/`template_id`/
   `reuse_from` to Task model+row; an `instantiate` helper (template → Task);
   expose all on `StoreHandle`. Unit-test each verb.
2. **Engine**: an inheritance resolver (`EffectiveGit { repo, base_branch,
   branch_prefix, worktree_mode }`) computed from `(task, Option<Run>, EngineConfig)`;
   plug it into `scheduler/worktree.rs::provision`; honour `reuse_from`. Unit-test
   the resolver in isolation.
3. **API**: the `/templates` and `/workflows` routes + DTOs, wired into the router.
4. **Integration test**: the 3-concurrent-workflows REST test above.
5. Build + clippy + test; commit in scoped commits on the current branch.
