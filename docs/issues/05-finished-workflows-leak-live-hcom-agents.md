# Finished workflows leak live hcom agents (no teardown on completion)

**Labels:** bug, engine, scheduler, resource-leak
**Severity:** high — accumulating live agents starve the event drain and stall *other* workflows

## Summary

When a workflow reaches `state=done` it is **not** torn down: its `lifecycle` stays
`active`, and any agents it spawned (especially the `auto_pr` agent — see issue #06) keep
running as live hcom processes. Nothing reaps them. Over time a "finished" workflow leaves
a pile of live agents behind, and because hcom's event drain has a fixed cap, a large
enough pile **starves the event stream for every other workflow** — DONE/gate events get
skipped, and unrelated running tasks park at `gating`/`running` forever.

The operator's mental model is "the workflow finished, it's gone." The system's reality is
"finished workflows linger as `active` and keep agents alive." There is no automatic
"workflow complete → stop + reap agents" step.

## Observed

`hcom list` while only the `projects` workflow was being worked on:

```
live agents: 185
    179  doc-writer-autopr     <- doc-writer was state=done, 5/5, for hours
      1  proj-design / proj-store / proj-api / proj-ui (stale, from a done run)
```

`doc-writer` had finished (`done 5/5`) but was still `lifecycle=active`; its leaked
auto_pr swarm (issue #06) flooded the drain:

```
WARN lazybones_engine::hcom::events: tail: drain hit --last cap;
     events older than this batch may be skipped  cursor=941 cap=5000
```

That starvation is what made the **unrelated** `projects` tasks stall at `gating` — the
operator chased a "stuck projects workflow" that was actually fine; the culprit was a
different, already-finished workflow's leaked agents.

Snapshot of done-but-active workflows (all candidates to leak):

```
doc-writer               lifecycle=stopped  state=done  auto_pr=True  done=5/5
projects                 lifecycle=stopped  state=done  auto_pr=True  done=5/5
rubix-device-management  lifecycle=active   state=done  auto_pr=None  done=7/7
simple-demo              lifecycle=active   state=done  auto_pr=True  done=3/3
web-demo-3               lifecycle=active   state=done  auto_pr=None  done=5/5
web-demo-4               lifecycle=active   state=done  auto_pr=None  done=5/5
```

Even after `projects` was *stopped*, `hcom list` still showed `projects-autopr` agents
live — stop did not fully reap them either.

## Root cause (suspected)

- Workflow completion stamps `finished_at` / `state=done` but never flips `lifecycle` away
  from `active`, and never issues a stop/reap of the run's agents. The only paths that kill
  agents are explicit operator `stop`/`restart` and per-task teardown — there is no
  **on-workflow-complete** hook.
- `stop` reclaims tasks but does not hard-kill detached headless agents promptly; they
  remain in `hcom list` (observed: `projects-autopr` survived a stop, and the
  `doc-writer-autopr` swarm had to be killed by name via `hcom kill`).
- hcom's drain cap (`cap=5000`) is a fixed ceiling with no backpressure or per-workflow
  fairness, so one workflow's leaked agents degrade global event delivery.

## Fix direction

- On workflow completion (last task `done`): transition `lifecycle` to a terminal state
  (e.g. `completed`), and **reap every agent tagged to the run** (`hcom kill tag:<run>`),
  including the auto_pr agent.
- `stop`/`restart` should **confirm** agents are gone (poll `hcom list`, force-kill
  stragglers) rather than fire-and-forget.
- Consider a reaper sweep: any agent whose task is `done`/whose workflow is terminal and
  that has been idle/stale for >N seconds gets killed.
- Make the event drain resilient to a noisy workflow (per-run cursor or fairness) so one
  run can't starve others — at minimum, surface `drain hit cap` as an operator-visible
  health warning, not just a log line.

## Impact

Leaked agents accumulate silently and, past the drain cap, cause **cross-workflow
stalls** that look like a bug in the innocent workflow. Operators waste time triaging the
wrong run. There is also a real resource leak (processes, ptys, memory) on long-lived
daemons.
