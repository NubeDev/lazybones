# The default workflow — author one the way we always want it

> Audience: an AI agent (or a human driving one) asked to **set up a workflow** for
> review. This is the **one canonical recipe**. Follow it verbatim — these are
> settled house defaults, **not** per-request choices. Do **not** ask the user to
> pick worktree mode, model, effort, or retry policy; they are fixed below. Only ask
> about things genuinely specific to *their* task (which repo, what the tasks are).
>
> Companion docs: [`managing-with-ai.md`](../managing-with-ai.md) (the full REST/MCP
> playbook — every verb), [`mcp/README.md`](../mcp/README.md) (the MCP surface this
> uses).

## TL;DR — the defaults, baked in

Every workflow you author for a human gets **exactly** these, unless they
explicitly say otherwise:

| Setting | Value | Why |
|---|---|---|
| **Worktree mode** | **`shared`** ("Shared (one PR)") | One new worktree + **one branch** for the whole workflow; every task builds on the previous one's commits in the same tree → **one PR**. |
| **Model** | `claude-opus-4-8` | |
| **Effort** | `medium` | |
| **Agent (tool)** | `claude` | |
| **Auto-retry** | `long_term`, `max_retries: 2` (on **every** task) | A block self-heals up to 2× before it waits for a human. |
| **Start?** | **No — hand back idle.** | The human reviews the tasks and presses Start themselves (see house rule below). |

Set these on the **workspace** (so all tasks inherit) **and** the auto-retry on each
task. That's the whole recipe.

## The one rule about `shared` (read this — it's where people get confused)

`shared` is a **first-class worktree mode**, not a manual chain.

- Setting `workspace.worktree_mode = "shared"` makes **the whole workflow** run on
  **ONE branch + ONE worktree**, named from the **run (workflow) id**. Tasks run
  sequentially in that same tree and build on each other. The workflow yields a
  **single branch → single PR**.
- **Do NOT** set `worktree_mode_override: "reuse"` or `reuse_from` on the tasks to
  "make them share." That is the **wrong** mechanism — a per-task `reuse` override
  *fights* the workflow's `shared` mode and is what makes the UI show one task as
  "Shared" and the rest as "Reuse." For a default workflow, tasks carry **no
  worktree override at all** — they inherit `shared` from the workspace.
- `shared` also **forces the merge strategy to `pr`** in the engine
  (`scheduler/effective.rs::merge_for`) — auto-merging task branches mid-run would
  defeat "one shared branch → one PR." So `shared` ⇒ one PR, by construction.

> Resolution rule (from `scheduler/effective.rs`):
> `worktree_mode = task.worktree_mode_override ?? workspace.worktree_mode ?? global`.
> Leave the task override **unset** and set the workspace to `shared`. Done.

Optionally also set `workspace.auto_pr = true` so the daemon opens that one PR
automatically when the run finishes (the empty/merged-branch spawn-loop guard,
issue #06, is fixed — it's safe). Otherwise the human opens the single PR.

## House rule (unchanged): authoring is not running

**Do NOT start the workflow.** Author the run + tasks + auto-retry, then **stop and
hand back**. The human presses Start (`workflow.start` / `POST /workflows/:id/start`)
after reviewing. Only start it if they explicitly say "start it / run it now." There
is **no** per-workflow "bypass" knob — permission mode is the daemon-global `auto`;
tell the user `auto` already covers it. (See `managing-with-ai.md` §"House rules.")

---

## Recipe A — over MCP (preferred; typed tools)

Connect with an **authoring** token (see `managing-with-ai.md` §6a–6b; the default
`author` profile can create everything but **cannot** start/stop/delete — exactly
right). Then:

1. **`workflow.create`** with the workspace defaults:
   ```jsonc
   { "id": "<wf-id>", "title": "<title>",
     "workspace": {
       "repo": "/abs/path/to/repo",
       "base_branch": "master",
       "branch_prefix": "lazy/<wf-id>/",
       "worktree_mode": "shared",     // ← the default. one branch, one PR.
       "tool":  "claude",
       "model": "claude-opus-4-8",
       "effort": "medium",
       "auto_pr": true                // optional: open the one PR at the end
   } }
   ```
2. **`workflow.add_task`** once per task — spec + `deps` only. Chain `deps` so tasks
   run in order (`t2.deps=["t1"]`, …). **No `worktree_mode_override`, no
   `reuse_from`** — they inherit `shared`. Tool/model/effort inherit the workspace,
   so you may omit them too.
3. **`task.auto_retry`** on **every** task: `{ "strategy": "long_term", "max_retries": 2 }`.
4. **Hand back.** Do not call `workflow.start` (your token can't anyway).

## Recipe B — over REST (curl)

```sh
BASE=http://127.0.0.1:46787; AUTH="Authorization: Bearer lazybones-loop"; JSON="Content-Type: application/json"
WF=my-feature

# 1. workflow with the defaults (shared → one branch → one PR)
curl -X POST $BASE/workflows -H "$AUTH" -H "$JSON" -d '{
  "id":"'$WF'","title":"My feature",
  "workspace":{"repo":"/abs/repo","base_branch":"master","branch_prefix":"lazy/'$WF'/",
    "worktree_mode":"shared","tool":"claude","model":"claude-opus-4-8","effort":"medium","auto_pr":true}}'

# 2. tasks — deps chain them in order; NO worktree override, NO reuse_from
curl -X POST $BASE/workflows/$WF/tasks -H "$AUTH" -H "$JSON" -d '{"id":"t1","title":"Slice 1","spec":"...","deps":[]}'
curl -X POST $BASE/workflows/$WF/tasks -H "$AUTH" -H "$JSON" -d '{"id":"t2","title":"Slice 2","spec":"...","deps":["t1"]}'

# 3. auto-retry 2x on every task
for t in t1 t2; do
  curl -X PUT $BASE/tasks/$t/auto-retry -H "$AUTH" -H "$JSON" -d '{"strategy":"long_term","max_retries":2}'
done

# 4. STOP. Hand back idle — the human presses Start.
```

## Verify before handing back

```sh
curl $BASE/workflows/$WF | jq '{lifecycle, started_at, worktree_mode:.workspace.worktree_mode, auto_pr:.workspace.auto_pr}'
curl $BASE/tasks | jq -c '.[]|select(.run_id=="'$WF'")|{id,deps,worktree_mode,worktree_mode_override,reuse_from,model,effort,auto_retry,max_retries}'
```
Expect, on **every** task: `worktree_mode:"shared"`, `worktree_mode_override:null`,
`reuse_from:null`, `model:"claude-opus-4-8"`, `effort:"medium"`,
`auto_retry:"long_term"`, `max_retries:2`; and `started_at:null` on the workflow. If
any task shows `worktree_mode_override:"reuse"` you used the wrong mechanism — see
"The one rule about `shared`" above.

## Splitting a scope into tasks

One task per natural unit of the ask. If the user's brief already lists slices/parts,
make one task per slice/part. If a scope doc has numbered parts (e.g. agent-run's
Part 0–5), make one task per part, `deps`-chained in order. When in doubt, mirror how
the user described it — don't invent a finer or coarser breakdown than they gave.

## Common mistakes (don't repeat these)

- ❌ Leaving `worktree_mode` at the global default (`new` = "Isolated") and trying to
  fake sharing with per-task `reuse_from`. → ✅ Set `workspace.worktree_mode:"shared"`.
- ❌ Setting `worktree_mode_override:"reuse"` on tasks. → ✅ No task override; inherit.
- ❌ Asking the user to choose worktree mode / model / effort / retry. → ✅ Use the
  defaults in the TL;DR; only ask about repo + task content.
- ❌ Starting the workflow. → ✅ Hand back idle.
