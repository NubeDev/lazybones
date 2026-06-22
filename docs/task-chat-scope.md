# Task chat — talk to a task's agent, and workshop a failure back to green

> Status: backend shipped, UI next.
> Audience: whoever builds the chat UI on the workflow detail page.
> Read [workflows-scope.md](workflows-scope.md) for the Template / Workflow / Task
> nouns and [scheduler.md](scheduler.md) for the tick loop.

## The user story this serves

> 1. I open a workflow's task and see the conversation between me and its agent.
> 2. While the task is **running**, I can message the agent to steer it.
> 3. When a task **fails (blocked)**, I message the agent to workshop the fix —
>    that *revives* the task: the loop re-spawns an agent in the kept worktree with
>    our whole conversation in front of it, and drives it back through the gate.

This is the lighter-weight alternative to the watcher in
[supervisor-scope.md](supervisor-scope.md): instead of a separate supervisor agent
reporting on the workflow, the operator talks to the task agent directly.

## How it works (the mechanism already in the tree)

Every task agent listens on an **hcom thread named after its task id** (that is how
it already signals `DONE`/`BLOCKED`, see `scheduler::prompt`). Chat reuses that:

- **Operator → agent** is `hcom send @all --thread <task-id> -- <text>`
  (`lazybones_engine::send_to_agent`), reaching a *running* agent live.
- **Agent → operator** replies land on the same thread, are captured by the
  existing `hcom_tail` into the `hcom_log`, and are **mirrored into the `chat`
  store** as `role = agent` (the agent's `DONE`/`BLOCKED` control lines are
  filtered out — they are lifecycle, not chat).

The `chat` store (a new append-only table, keyed on `task`) is the single durable
source of the conversation, independent of hcom's transcript retention. Operator
messages are written there directly by the POST route; agent replies are mirrored
in (deduped on `(task, hcom_id)`).

## When a message is acted on, by task state

`POST /tasks/:id/chat` always stores the message first, then:

| Task state           | What happens                                                            | `delivery` |
| -------------------- | ---------------------------------------------------------------------- | ---------- |
| `running` / `gating` | sent live on the hcom thread to steer the agent                        | `delivered` (or `stored` if the live send fails) |
| `blocked`            | **revived**: `blocked -> ready`, worktree kept; next tick re-spawns it | `revived`  |
| `pending` / `ready`  | stored as guidance; folded into the prompt at the first claim          | `stored`   |
| `done`               | rejected `409` — restart the task to re-run it                         | —          |

Revive is a real lifecycle edge (`Transition::Revive`, the only edge out of
`blocked`): it clears the block reason and the dead agent's session but **keeps the
worktree/branch**, so the re-spawned agent resumes in place. The prompt
(`scheduler::prompt::compose`) folds the whole conversation in under an
`=== OPERATOR CONVERSATION ===` heading so the agent sees the guidance before it
re-attempts.

## REST surface

| Method · path | Job | Auth |
| --- | --- | --- |
| `GET /tasks/:id/chat` | the conversation, oldest first — `Vec<ChatMessage>` | open (like the hcom log / transcript reads) |
| `POST /tasks/:id/chat` | post a message (`{ "text": "…" }`) → `{ message, delivery }` | `Block` capability (the operator task-control cap) |

`ChatMessage` is `{ run, task, role: "user" | "agent", text, at }`.
`delivery` is `"delivered" | "revived" | "stored"` (see the table above).

## Live updates (for the UI)

`GET /stream` (SSE) now emits a **`chat`** event whose `data` is a `ChatMessage`,
for both operator messages and mirrored agent replies — so a chat panel updates
without polling. Reconcile on (re)connect by refetching `GET /tasks/:id/chat`
(the durable rows are the source of truth; the stream only carries what occurs
while connected).

## UI plan (next pass)

- A **Chat** panel/tab on the task view (workflow detail), rendering the
  `ChatMessage` feed as left/right bubbles by `role`, with a composer that POSTs.
- Subscribe to the `chat` SSE event for live append; refetch on reconnect.
- Surface the `delivery` result: e.g. a "revived — re-running" toast when a blocked
  task is workshopped, a subtle "sent" for a live steer.
- Disable the composer (or show "restart to re-run") when the task is `done`.

## Non-goals (this pass)

- **No new task states** and no change to the gate/done-definition — revive reuses
  the existing `ready` claim path.
- **No loop-breaking cap** on operator-driven revives yet: unlike the
  *acting supervisor* in [supervisor-scope.md](supervisor-scope.md), a human is in
  the loop here, so the runaway-redo risk is bounded by the operator. If chat ever
  drives automated re-tries, add the per-task redo cap that doc calls for.
