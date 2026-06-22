# Multi-repo scope & worktree control

> Status: decision record. No build work implied.
> Audience: anyone tempted to add a `repo` field to `Task`, or to make tasks
> share one worktree per workflow.
> Read [workflows-scope.md](workflows-scope.md) first for the Template /
> Workflow (= Run) / Task nouns and the git-config inheritance rule. This doc
> records two design decisions that sit on top of that model.

## TL;DR

1. **A workflow stays single-repo.** The repo lives on the workflow's
   `workspace`, not on the task. Cross-repo work is done with **multiple
   workflows**, not one multi-repo workflow.
2. **Tasks do not share "the workflow's worktree."** A workflow has a *repo*,
   not a worktree. Each task materialises its **own** worktree from that repo.
   Sharing a tree is a per-edge decision (`reuse_from`), driven by data
   dependency — never implied by "same repo".

The mental model: **workflow : repo :: task : worktree.** One repo, many
worktrees.

## Decision 1 — workflows are single-repo; use multiple workflows for multi-repo

### The question
Should one workflow be able to span several repos (e.g. an `api` repo and a
`ui` repo), so a single DAG can express "change the API, then consume it in the
UI"?

### The decision: no — keep one repo per workflow
The repo is a single field on `Workspace` ([run/model.rs:23](../crates/lazybones-store/src/run/model.rs#L23))
and it stays that way. When work touches more than one repo, the user makes
**one workflow per repo** and runs them concurrently. The scheduler already
runs N workflows at once against one global concurrency budget with per-run
fairness ([workflows-scope.md](workflows-scope.md) §Concurrency), so this costs
nothing extra.

### Why this is the right call (not just the easy one)

- **The thing that would justify a multi-repo workflow is a dependency edge
  crossing a repo boundary** (`ui-task depends_on api-task`). In practice those
  edges are rare, and when they exist the coupling is usually "merge repo A,
  *then* start repo B" — which is sequencing two workflows, not interleaving one
  DAG. Humans (or a thin outer trigger) sequence them; the engine stays simple.
- **Putting `repo` on the task is the trap to avoid.** It scatters repo identity
  across N tasks, makes "what does this workflow touch?" un-queryable, breaks the
  clean `task ?? workspace ?? global` inheritance chain
  ([effective.rs:44](../crates/lazybones-engine/src/scheduler/effective.rs#L44) — `resolve()`),
  and leaves nowhere to hang per-repo config (base branch, prefix, remote). It
  denormalises the one thing that is currently nicely normalised.
- **Multiple single-repo workflows lose nothing** the engine actually provides.
  Isolation, gating, fairness, and reuse all work per-workflow already.

### The seam we leave (if multi-repo ever earns its place)
Do **not** add `repo` to `Task`. If a genuine cross-repo DAG ever becomes
necessary, evolve the *workspace*, not the task:

```text
Workspace.repo: string
  -->  Workspace.repos: map<key, RepoBinding>   # "api" -> ..., "ui" -> ...
       Workspace.default_repo: string

Task gains:  repo_key?: string                  # None -> workspace.default_repo
```

This is a pure superset: a single-repo workflow is one entry in `repos` with
`repo_key = None`, so existing workflows need zero migration. `EffectiveGit`
resolution gains exactly one hop (`task.repo_key -> workspace.repos[key] ->
global`) and keeps its most-specific-wins shape. **We are not building this** —
it is recorded only so the future move is the workspace, never the task.

## Decision 2 — worktree control follows data dependency, not repo identity

### The question
If two tasks are on the same repo/dir, should they share one worktree (the
"workflow's worktree")?

### The decision: no shared-by-default; per-task isolation is the default
There is no single "workflow worktree" to share. Each task provisions its own
tree via `WorktreeMode`
([task/model.rs:16](../crates/lazybones-store/src/task/model.rs#L16)), applied in
`provision()` ([worktree.rs:45](../crates/lazybones-engine/src/scheduler/worktree.rs#L45)).
Tree sharing happens **only** on an explicit `reuse_from` edge.

### Why "same repo" is *not* a reason to share a tree
"Same repo" is precisely the situation isolation exists for. Two tasks writing
the same checkout race each other: half-committed trees, the gate running
against the wrong state, and lost parallelism. The `owns` collision-guard field
on `Task` ([task/model.rs:68](../crates/lazybones-store/src/task/model.rs#L68)) already
assumes multiple tasks touch one repo in parallel — that only works because each
gets its own tree.

### The rule, mapped to the modes that already exist

| Mode | When | Tree | Parallel? |
| --- | --- | --- | --- |
| `New` (default) | normal task | fresh worktree off `base_branch`, merged on green | yes |
| `Reuse` + `reuse_from` | task **continues** another task's tree (B builds on A) | A's stored `worktree` path | no — B blocks until A's tree exists |
| `Branch` | worktrees disabled in the environment | the main checkout | no — serial fallback |

- **`Reuse` is the only legitimate "share a worktree" case**, and it is
  necessarily serial. It is keyed on `reuse_from`
  ([task/model.rs:98](../crates/lazybones-store/src/task/model.rs#L98)) — a *data*
  relationship — not on the two tasks happening to share a repo. **A `reuse_from`
  that names a known task folds into `deps`**: whichever path creates the task
  (workfile sync or `POST /workflows/:id/tasks`) adds the source to the dependency
  set via `deps_with_reuse`
  ([sync.rs](../crates/lazybones-store/src/workfile/sync.rs)), so the readiness
  graph holds B back until A is `done` and the plan graph draws the edge. One
  source of truth — `deps`, readiness, and the graph all agree; there is no
  parallel reuse-edge to keep in sync.
- **An *unknown* `reuse_from` source is deliberately not folded.** A typo, or a
  source in another workflow, isn't a task this run's readiness query can resolve
  — folding it would wedge the task `pending` forever on a ghost dep. Those cases
  fall to the claim-time guard `resolve_reuse`
  ([tick.rs:186](../crates/lazybones-engine/src/scheduler/tick.rs#L186)), which
  blocks B with a reason naming the missing source. So: in-workflow reuse is a
  graph dependency; cross-workflow reuse is a claim-time wait.
- **The default stays `New`.** Don't collapse tasks onto one tree because they
  share a repo; collapse them only when one literally consumes the other's
  uncommitted work.

### One gap multi-repo *would* expose (and why we don't build it now)
The gate runs in the task's worktree
([gate.rs:30](../crates/lazybones-engine/src/scheduler/gate.rs#L30)). A real
cross-repo change would eventually want a **workflow-level gate** that runs after
the cross-repo merges (e.g. an integration test spinning up API + UI together).
Since Decision 1 keeps workflows single-repo, this need does not arise yet — so
we don't build a workflow-level gate speculatively. It is noted here as the one
new execution concept a future multi-repo move would force.

## Net effect on the current code

Both decisions ratify what the model already does:

- repo on `workspace`, not `task`;
- `EffectiveGit` resolves `task ?? workspace ?? global`;
- `WorktreeMode::New` default, `Reuse` gated behind `reuse_from`.

One coupling was added to honour Decision 2's intent: **a known `reuse_from`
source now folds into `deps`** (shared `deps_with_reuse` helper) on both creation
paths — workfile sync and the add-task route — and `SeedTask` carries `reuse_from`
so the workfile can express it. This moves in-workflow reuse ordering into the
readiness graph (visible in the UI plan graph and deps list) instead of leaning
solely on the claim-time block; unknown/cross-workflow sources still rely on that
block. The add-task dialog reflects this: picking an in-workflow reuse source
shows it as an implied, locked dependency.

This doc exists so the next person reaches for **another workflow** (multi-repo)
or **`reuse_from`** (shared tree) instead of a `Task.repo` field or a shared
"workflow worktree" — the two changes that would quietly erode the design.
