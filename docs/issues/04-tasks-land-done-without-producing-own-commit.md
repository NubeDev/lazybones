# Tasks reach `done` without producing their own commit (false completion)

**Labels:** bug, engine, scheduler, data-integrity
**Severity:** critical — a workflow reports "5/5 done" while 3 of the tasks did no work; the lie is silent

## Summary

In a **shared-worktree** workflow, a task whose agent dies/parks **without doing any
work** still transitions to `done` — stamped with the *previous* task's commit. The
workflow then shows `state=done`, `done_count=N/N`, and opens a PR, but the branch only
contains the commits of the tasks that genuinely ran. There is **no signal** to the
operator that later tasks were empty: the API says done, the PR exists, everything looks
finished.

This is the core failure behind the `projects` workflow: `proj-design` and `proj-store`
committed real work; `proj-api`, `proj-ui`, and `proj-pr` were all marked `done` against
`proj-store`'s commit (`85a9cae`) having produced **zero** files.

## Observed

`projects` workflow (5 tasks, `worktree_mode: shared`, branch `lazy/projects`):

```
proj-design  done  commit=78dff40   ✅ wrote docs/lazybones-server/projects-decisions.md
proj-store   done  commit=85a9cae   ⚠️ PARTIAL — only crates/lazybones-store/src/org/
proj-api     done  commit=85a9cae   ❌ SHARED commit — zero route files produced
proj-ui      done  commit=85a9cae   ❌ SHARED commit — zero feature folders produced
proj-pr      done  commit=85a9cae   opened PR #3 of the incomplete branch
```

`git log origin/master..origin/lazy/projects` → **2 commits**, not 5. Yet the workflow
reported complete and PR #3 was opened as if the feature were built.

## Root cause

`crates/lazybones-engine/src/scheduler/finish.rs`, `gate_and_land()` (~lines 312–335):

```rust
gate::GateOutcome::Green => {
    match merge::commit_worktree(wt, task, progress).await {
        Ok(Some(_sha)) => {}      // committed → land
        Ok(None) => {
            // clean tree, nothing to commit. Is this a real no-op?
            let has_commits = merge::branch_has_commits(wt, &eff.base_branch, &branch)
                .await.unwrap_or(true);
            if !has_commits {
                block(store, task, "task produced no changes to commit (empty task)"...).await;
                return;
            }
            // "Clean tree but the agent already committed — land those commits."
        }
        ...
    }
    // land() stamps Done { commit: <current HEAD> }
```

The empty-task guard asks **"does the branch have commits ahead of base?"** — but in a
shared worktree the branch **already carries every prior task's commits**. So for task 3
of 5, `branch_has_commits` is `true` even when task 3 itself produced nothing. The guard
is satisfied, control falls through to `land()`, and the task is stamped `Done` with the
*current* HEAD — i.e. the previous task's sha. An empty task is indistinguishable from a
task that legitimately added no new commit on top of work it shares.

The guard answers the wrong question. In shared mode the right question is **"did *this
task* advance HEAD beyond where it started?"**, not "are there any commits ahead of base?"

## Fix direction

- Capture each task's **starting HEAD** at claim time (the worktree HEAD when the agent
  is spawned), persist it on the task (e.g. `base_commit`).
- At gate, the empty-task check becomes `HEAD == base_commit` → genuinely produced no
  work. Then either **block** ("empty task") or, if empty tasks are legal, mark done with
  an explicit `commit=null` / `empty=true` flag — never silently inherit the prior sha.
- The `Done { commit }` transition should reject (or at least warn loudly + flag) a commit
  equal to a sibling task's commit in the same run, so "shared commit across N done tasks"
  surfaces instead of passing as success.
- Liveness caveat compounds this: a task can reach this path via the `AgentDone`
  (idle/exited, no DONE posted) signal — see finish.rs `Signal::AgentDone`. A dead agent
  that did nothing should block, not "gate its (nonexistent) work."

## Impact

Silent, data-loss-shaped: the operator is told a feature is built when most of it was
never attempted. A PR is opened against the incomplete branch, inviting a merge of
half-built work. This is the most dangerous class of bug — the system lies about success.
