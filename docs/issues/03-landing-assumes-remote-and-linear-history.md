# Landing assumes a pushable remote and non-diverging branches

**Labels:** bug, engine, scheduler
**Severity:** high — every parallel workflow and every remote-less repo fails to land

## Summary

After a task's gate passes, `merge::land` integrates the task branch into base and
**pushes**. Two assumptions break real use:

1. **A remote always exists.** `push()` runs `git push <remote> <ref>` unconditionally
   and bails on failure. On a local target repo with no `origin`, every task fails:
   `git push origin master failed: fatal: 'origin' does not appear to be a git repository`.
2. **Base hasn't moved (linear history).** Default merge mode `fast-forward` uses
   `git merge --ff-only`. Two parallel tasks both branch from `master`; once the first
   lands, the second can't fast-forward:
   `Diverging branches can't be fast-forwarded ... Not possible to fast-forward, aborting`.
   Parallel tasks **always** diverge, so `fast-forward` is unusable for concurrent work.

A gate-passing task then blocks at the land step — silent data-loss-shaped: the work is
committed on the `lazy/<task>` branch but never reaches base.

## Observed

`web-demo-2` (4 tasks, concurrency 3, local Node repo, no remote):
- `backend` → `merge failed: git push origin master failed: 'origin' does not appear to be a git repository`.
- `frontend` → `git merge of lazy/frontend into master failed: ... Not possible to fast-forward`.
- Dependent tasks stranded; workflow `needs-attention`, 0 landed.

## Where

`crates/lazybones-engine/src/scheduler/merge.rs` — `land()` and `push()`.

## Fix applied

1. **`push()` skips a missing remote instead of failing** — probe
   `git remote get-url <remote>`; if absent, `warn!` and return Ok (landed locally).
   A *configured* remote that genuinely fails to push still errors.
2. **Default merge mode → `merge`** (`lazybones.yaml`): diverged parallel branches
   integrate via a merge commit. `fast-forward` is now opt-in for serialized runs.

```rust
async fn push(repo, remote, refname) -> Result<()> {
    let has_remote = git(repo, &["remote", "get-url", remote]).await.map(|o| o.ok).unwrap_or(false);
    if !has_remote { warn!(...); return Ok(()); }     // local land is valid
    // ...push and propagate real errors
}
```

## Result

`web-demo-2` then landed **4/4 green** with merge commits, all locally (no remote).

## Follow-ups to consider

- Make merge mode resolvable per-workflow (mirror `workspace.gate` from issue 02), so
  a repo wanting strict linear history can keep `fast-forward` while others use `merge`.
- For `ff-only`, consider an automatic fall-back to a merge commit on divergence rather
  than hard-blocking (configurable).
- Serialize the `git worktree add` / land critical section if concurrent git ops on the
  same repo prove racy under high concurrency.
