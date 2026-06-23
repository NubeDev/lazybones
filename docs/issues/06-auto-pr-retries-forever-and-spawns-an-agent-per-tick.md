# auto_pr retries forever on an empty/merged branch, spawning an agent every tick

**Labels:** bug, engine, scheduler, resource-leak
**Severity:** high — an unkillable spawn loop; the source of the agent swarm in issue #05

## Summary

A workflow with `auto_pr: true` whose branch **cannot produce a PR** (already merged, or
no commits between base and head) retries the PR open **on every scheduler tick** and
**spawns a fresh agent each time without reaping the previous one**. The branch never
becomes PR-able, so the loop never terminates. Within an hour this produces ~180 live
agents from a single finished workflow — the swarm behind issue #05's cross-workflow
stalls.

## Observed

`doc-writer` (`state=done 5/5`, `auto_pr=true`, branch `lazy/doc-writer` already merged via
PR #2, `pr_url=null`). Every tick, in the daemon log:

```
WARN lazybones_engine::scheduler::auto_pr: auto_pr: open PR failed (will retry next tick):
  gh pr create --head lazy/doc-writer --base master ... exited with exit status: 1:
  GraphQL: No commits between master and lazy/doc-writer, Head ref must be a branch
```

…repeated indefinitely, while `hcom list` climbed:

```
60 doc-writer-autopr   (then 142, then 179 live)
```

Killing the swarm required `hcom list | grep -oE 'doc-writer-autopr-[a-z]+' | sort -u |
xargs hcom kill`, and the workflow had to be **stopped** to halt the spawn loop — it would
otherwise rebuild the swarm after every daemon restart.

## Root cause

`crates/lazybones-engine/src/scheduler/auto_pr.rs`:

1. **No terminal failure / no backoff.** "open PR failed → will retry next tick" treats
   *every* failure as transient. But "No commits between base and head" / "already merged"
   is **permanent** — retrying can never succeed. There is no cap, no backoff, no
   give-up-and-block.
2. **Spawn without reaping.** Each attempt spawns a new `<run>-autopr` agent; the prior
   attempt's agent is not killed first, so attempts accumulate as live agents rather than
   replacing one another.
3. **No empty-branch precheck.** auto_pr does not check `branch_has_commits(base, head)` /
   existing-PR / merged-state before spawning — so it launches an agent for a PR that is
   structurally impossible.

This compounds issue #05: a finished workflow that is never torn down keeps `auto_pr`
eligible forever, so the loop runs for the entire lifetime of the daemon.

## Fix direction

- **Precheck before spawn:** if there are no commits between base and head, or the branch
  is already merged, or a PR already exists for the head ref → do **not** spawn; mark the
  auto_pr step satisfied/skipped (with a clear reason), not "retry next tick."
- **Classify failures:** permanent gh errors (`No commits between`, `already exists`,
  `Head ref must be a branch`) are terminal → stop retrying and surface the reason; only
  genuinely transient errors (network, rate-limit) retry, with **bounded backoff + a cap**.
- **Reap before respawn:** kill the previous `<run>-autopr` agent before launching a new
  attempt; never let attempts accumulate.
- **Gate on lifecycle:** a `done`/terminal workflow (issue #05) should not keep an active
  auto_pr loop at all.

## Impact

A single misconfigured/finished workflow becomes an unbounded agent spawner that survives
restarts, leaks processes, and — via the drain cap — stalls unrelated workflows. It turned
a routine "open a PR for a merged branch" into the multi-hour incident that masked the real
state of the `projects` workflow.
