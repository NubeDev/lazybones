# Managing lazybones with an AI agent

> Audience: an AI agent (or a human driving one) operating lazybones over its REST
> API. This is an operator playbook — how to CRUD workflows/templates/tasks,
> supervise running tasks, set up and test agents, and read hcom logs.
>
> Out of scope here: the supervisor design (`supervisor-scope.md`) is not built yet
> — ignore it.

## TL;DR

lazybones is a durable task queue + green-build gate with an in-process scheduler.
You drive it entirely over HTTP. The split that matters:

- **You** = the control plane. You author and **promote** tasks; you never mark a
  task `running` yourself.
- **The scheduler** (a Rust loop inside `lazybonesd`) = the execution plane. It
  promotes ready tasks, provisions worktrees, claims them (`ready → running`),
  spawns the agent via hcom, runs the gate, and lands the branch.

A task runs only when **both** happen: you promote it **and** the daemon is up.

## House rules when authoring a workflow for a human (read first)

When a human asks you to *set up* a workflow (author the run + its tasks) so they
can review it, follow these — they are settled defaults, not per-request choices:

- **Do NOT start the workflow.** Author the workflow, its tasks, and any
  auto-retry policy, then **stop and hand back** — the human reviews the tasks and
  presses Start (`POST /workflows/:id/start`) themselves. Only start it if they
  explicitly ask you to ("start it", "kick it off", "run it now"). A freshly
  authored workflow is `active` but has promoted nothing, so it sits idle and safe
  until Start.
- **Permission mode is `auto`, and it is daemon-global — there is no per-workflow
  or per-task "bypass" field.** The API silently ignores a `permission_mode` /
  `bypassPermissions` key in the workspace; the engine spawns `claude` with
  `--permission-mode auto` (a classifier auto-approves safe calls). This is
  deliberate: true bypass (`--dangerously-skip-permissions`) triggers a one-time
  consent screen in claude v2.1.x that **hangs headless agents** (see
  `crates/lazybones-engine/src/config.rs` and the `headless-agent-stuck-bypass-consent`
  note). `auto` is the mode that has run prior workflows (e.g. `doc-writer`)
  cleanly. If a user asks for "bypass," tell them it's already effectively handled
  by `auto` and that switching to real bypass means reconfiguring + restarting the
  daemon (`permission_flags` config), at the risk of the consent-screen hang — do
  not silently try to set it on the workflow.

## Connection facts

| Thing | Value |
|---|---|
| Base URL | `http://127.0.0.1:46787` (env `LAZYBONES_BIND`) |
| Loop token | `lazybones-loop` (env `LAZYBONES_LOOP_TOKEN`) — full write access |
| Auth header | `Authorization: Bearer lazybones-loop` |
| Errors | JSON `{ "error": "..." }`; 401 unauth, 403 missing capability, 404, 409 conflict, 400 bad request |

Convention for every example below:

```sh
BASE=http://127.0.0.1:46787
AUTH="Authorization: Bearer lazybones-loop"
JSON="Content-Type: application/json"
```

GET reads (`/tasks`, `/workflows`, `/templates`, `/runs/*`, `/stream`, `/engine`,
`/health`) are open and need no token. All writes need the loop token.

Status values (serialized lowercase): `pending → ready → running → gating → done`;
any non-terminal state can go to `blocked`; a stale `running` task is reclaimed to
`ready`.

---

## 1. CRUD

### 1a. Tasks

Tasks are the unit of work. Standalone tasks live outside any workflow; workflow
tasks carry a `run_id` (§1c).

**Create** (`POST /tasks`, lands in `pending`):
```sh
curl -X POST $BASE/tasks -H "$AUTH" -H "$JSON" -d '{
  "id": "lint",
  "title": "Fix clippy warnings",
  "spec": "Run cargo clippy --workspace and fix all warnings.",
  "deps": [],
  "owns": ["crates/lazybones-store"],
  "tool": "claude",
  "worktree_mode": "new"
}'
```
- `deps` = task ids that must be `done` before this can promote.
- `owns` = path globs this task is allowed to touch (conflict avoidance).
- `worktree_mode` = `"new" | "reuse" | "branch"` (default `new`).

**Read:**
```sh
curl $BASE/tasks                       # all
curl "$BASE/tasks?status=ready"        # filter by status
curl $BASE/tasks/lint                  # one task (404 if absent)
```

**Update** (`PATCH /tasks/:id` — overwrites authored fields, reconciles deps):
```sh
curl -X PATCH $BASE/tasks/lint -H "$AUTH" -H "$JSON" -d '{
  "title": "Fix clippy warnings (workspace)",
  "spec": "Run cargo clippy --workspace --all-targets and fix all -D warnings.",
  "deps": [], "owns": ["crates"], "tool": "claude"
}'
```

**Delete** (`DELETE /tasks/:id` — also removes dep edges):
```sh
curl -X DELETE $BASE/tasks/lint -H "$AUTH"   # → { "deleted": true }
```

**Promote** (the human "go" signal — `pending → ready`):
```sh
curl -X POST $BASE/tasks/lint/ready -H "$AUTH"        # one task
curl -X POST $BASE/tasks/promote -H "$AUTH"           # every task whose deps are all done → [ids]
```
Promotion only succeeds when **all** deps are `done`; otherwise it's a no-op.

### 1b. Templates

Reusable task recipes. A template supplies a `spec_template` plus defaults; you
instantiate it into a workflow (§1c).

**Create** (`POST /templates`):
```sh
curl -X POST $BASE/templates -H "$AUTH" -H "$JSON" -d '{
  "id": "open-pr",
  "title": "Open a PR",
  "description": "Standard PR recipe",
  "spec_template": "Open a pull request for the completed work with a clear summary.",
  "default_tool": "claude",
  "default_model": "claude-opus-4-8",
  "default_effort": "high",
  "default_worktree_mode": "new"
}'
```

**Read / Delete:**
```sh
curl $BASE/templates                     # list
curl $BASE/templates/open-pr             # one (404 if absent)
curl -X DELETE $BASE/templates/open-pr -H "$AUTH"   # → { "deleted": true|false }
```

> There is **no** template `update` endpoint and **no** standalone "instantiate"
> endpoint. To edit a template, delete and recreate. To instantiate, add a workflow
> task with `from_template` (§1c).

### 1c. Workflows

A workflow (`Run`) binds a set of tasks to one workspace (a repo + git defaults).

**Create** (`POST /workflows`):
```sh
curl -X POST $BASE/workflows -H "$AUTH" -H "$JSON" -d '{
  "id": "ship-auth",
  "title": "Ship auth",
  "workspace": {
    "repo": "/home/user/code/myrepo",
    "base_branch": "main",
    "branch_prefix": "lazy/",
    "worktree_mode": "new",
    "tool": "claude",
    "model": "claude-opus-4-8",
    "effort": "high"
  }
}'
```
`workspace.repo` (absolute path) is the only required workspace field; the rest are
defaults inherited by tasks that don't override them.

**Add a task — authored inline** (`POST /workflows/:id/tasks`):
```sh
curl -X POST $BASE/workflows/ship-auth/tasks -H "$AUTH" -H "$JSON" -d '{
  "id": "auth",
  "title": "Add login",
  "spec": "Implement bearer auth in the API layer.",
  "deps": [], "owns": ["src/auth"],
  "tool": "claude", "model": "claude-opus-4-8", "effort": "high"
}'
```

**Add a task — from a template** (this is how you instantiate a template):
```sh
curl -X POST $BASE/workflows/ship-auth/tasks -H "$AUTH" -H "$JSON" -d '{
  "id": "pr",
  "title": "Open PR for auth",
  "from_template": "open-pr",
  "deps": ["auth"]
}'
```
With `from_template`, the template supplies `spec`/tool/model/effort defaults; any
field you also pass in the body overrides the template. `reuse_from` (a sibling task
id) and `worktree_mode_override` are available for sharing a worktree.

**Read:**
```sh
curl $BASE/workflows                 # list + derived state, task_count, done_count
curl $BASE/workflows/ship-auth       # detail + task_ids[]
```

**Start** (stamps `started_at`, promotes eligible root tasks to `ready`):
```sh
curl -X POST $BASE/workflows/ship-auth/start -H "$AUTH"   # → { "promoted": ["auth"] }
```

**Stop** (pause; lifecycle `stopped`, kills agents and reclaims running tasks to
`ready` — no work lost):
```sh
curl -X POST $BASE/workflows/ship-auth/stop -H "$AUTH"
```

**Stop & reset** (pause AND reset unfinished tasks to `pending`, discarding
in-flight progress; done tasks kept):
```sh
curl -X POST $BASE/workflows/ship-auth/stop-reset -H "$AUTH"
```

**Resume** (un-pause; lifecycle `active` + reset blocked tasks — the scheduler
picks back up):
```sh
curl -X POST $BASE/workflows/ship-auth/resume -H "$AUTH"
```

**Restart** (reset the workflow's tasks to `pending` to run from the beginning;
stays `active`, does **not** auto-start — press Start when ready):
```sh
curl -X POST $BASE/workflows/ship-auth/restart -H "$AUTH" -H "$JSON" -d '{
  "include_done": false,      # also reset done tasks (true = full from-scratch redo)
  "remove_worktrees": false   # also git-worktree-remove each reset task's tree
}'
```
Both flags default `false` (the safe, resume-style restart: keep done tasks, reuse
worktrees). Live agents are always killed first. Unlike **Stop & reset**, restart
leaves the run `active` and re-promotes immediately on Start rather than pausing.

**Delete** (the only archive/tombstone path; `409` if any task is still live —
stop first):
```sh
curl -X DELETE $BASE/workflows/ship-auth -H "$AUTH"   # → { "deleted": true|false }
```

> A `stopped` workflow promotes/claims nothing and refuses task-level retries
> (`409`) until resumed — so "stopped" never lies. Stop is reversible; lifecycle is
> only ever `active` or `stopped`, and `DELETE /workflows/:id` is the archive path.

---

## 2. Supervising & checking task stats

### Snapshot a task
```sh
curl $BASE/tasks/auth
```
Watch these fields: `status`, `heartbeat` (RFC3339 liveness ping), `session` (hcom
agent name / kill handle), `worktree`, `branch`, `commit` (set on `done`), `reason`
(set on `blocked`).

Timing stamps (RFC3339; `null` until reached, durations are derived, never stored):
`started_at` (first claim — kept across reclaims/revives), `finished_at` (on `done`),
`failed_at` (on `blocked`; cleared when revived or finished).

Retry-policy fields (see §2 "Revive a blocked task"): `auto_retry`
(`"long_term" | "quick" | null`), `max_retries` (cap, default `2`), `retry_count`
(hands-off attempts spent so far; zeroed on a clean reset/restart and on `done`).

### Full transition history (audit trail)
Every status change is a durable, queryable event. `run` is the event-grouping
label on a task (for a workflow task it's the workflow id).
```sh
curl $BASE/runs/<run>          # → [ {run, task, from, to, actor, at}, ... ] oldest first
```

### Live feed (SSE)
One connection, three event types. Reconcile by refetching the task list on
(re)connect — only events while connected are delivered.
```sh
curl -N $BASE/stream
```
- `transition` — durable lifecycle change `{run, task, from, to, actor, at}`
- `activity` — ephemeral progress message `{run, task, actor, message, at}`
- `hcom_log` — a raw hcom event just ingested `{run, task, agent, kind, data, at}`

### Intervene
```sh
# nudge a stuck task to blocked (keeps worktree for triage)
curl -X POST $BASE/tasks/auth/block  -H "$AUTH" -H "$JSON" -d '{"reason":"wrong approach"}'

# kill the live hcom agent, then block
curl -X POST $BASE/tasks/auth/cancel -H "$AUTH" -H "$JSON" -d '{"reason":"cancelled by operator"}'
```
You generally do **not** drive `claim`/`gate`/`done`/`heartbeat` by hand — the
scheduler and the spawned agent own those. They exist for the loop/agent, not the
operator.

### Revive a blocked task (retry / auto-retry / chat)

A `blocked` task is otherwise a dead end — the scheduler only ever promotes
`pending` tasks, so a failed one never re-enters the run on its own. These three
verbs put it back. All require the loop token and all **refuse with `409` if the
parent workflow is `stopped`** — resume the workflow first (§1c). Each is scoped to
one task id (`404` unknown).

**Retry** (`POST /tasks/:id/retry`) — two shapes, chosen by whether you send a
`strategy`:

```sh
# Clean retry (transient failure): kill any live agent, reset blocked → pending,
# clear the worktree/claim/reason and zero the auto-retry counter. Fresh start.
curl -X POST $BASE/tasks/auth/retry -H "$AUTH" -H "$JSON" -d '{
  "remove_worktrees": false        # true also git-worktree-removes the tree
}'

# Guided retry (it failed for a reason): revive in place (blocked → ready), KEEP
# the worktree, and fold the strategy's guidance into the re-spawn prompt so the
# agent builds on its partial work.
curl -X POST $BASE/tasks/auth/retry -H "$AUTH" -H "$JSON" -d '{
  "strategy": "long_term"          # "long_term" = fix the root cause properly
}'                                 # "quick"     = smallest change to go green
```
`409` if the task isn't `blocked` (a `done` task is finished — restart the workflow
to re-run it).

**Auto-retry policy** (`PUT /tasks/:id/auto-retry`) — durable, hands-off config: on
a block the scheduler re-attempts the task with the strategy's guidance, up to
`max_retries` times, before leaving it for a human. Manual retry is never capped (a
person is in the loop).
```sh
curl -X PUT $BASE/tasks/auth/auto-retry -H "$AUTH" -H "$JSON" -d '{
  "strategy": "quick",             # "long_term" | "quick" | null (null = turn off)
  "max_retries": 3                 # optional; omit to leave the cap unchanged (default 2)
}'
```
Sets `auto_retry`/`max_retries` on the task; the scheduler bumps `retry_count` each
hands-off attempt and stops when `retry_count >= max_retries`. `strategy: null`
disables it. This is config only — it never moves the task itself.

**Chat** (`GET`/`POST /tasks/:id/chat`) — converse with a task's agent. The post is
stored durably first, then acted on by the task's state:
```sh
curl $BASE/tasks/auth/chat                                   # the conversation, oldest first
curl -X POST $BASE/tasks/auth/chat -H "$AUTH" -H "$JSON" -d '{"text":"use the existing helper"}'
```
The response's `delivery` says what happened: `delivered` (sent live to a
running/gating agent on its hcom thread), `revived` (a `blocked` task was reopened —
the next tick re-spawns it in its kept worktree with this message in the prompt), or
`stored` (recorded as guidance, folded in at the next claim). `409` on a `done` task
(restart to re-run); `400` on empty text.

> **The two revive mechanisms** (don't conflate): a **clean reset** (`retry` with no
> strategy, `resume`, `restart`) sends `blocked → pending` and discards the
> worktree/counter — for transient/flaky failures. A **guided revive** (`retry` with
> a strategy, `chat` on a blocked task, or auto-retry) sends `blocked → ready`, keeps
> the worktree, and appends guidance to the prompt — for "it failed for a reason,
> here's how to fix it." `RetryStrategy` (`long_term | quick`) is the shared intent
> behind manual strategy-retry and hands-off auto-retry.

---

## 3. Setting up, managing & testing agents

Agents are external CLIs (`claude`, `codex`, `gemini`, `opencode`). lazybones needs
(a) the agent binary on PATH and (b) its API credential stored, encrypted, in the
store. The scheduler injects those credentials as env into every spawned agent.

### Is the engine (hcom) present?
```sh
curl $BASE/engine     # open: { installed, version, ... }
```

### Which agent CLIs are ready?
```sh
curl $BASE/agents -H "$AUTH"   # per tool: installed? credential stored? ready?
```
Detection also searches `~/.local/bin`, `~/.cargo/bin`, and Homebrew, because a
GUI-launched daemon often has a stripped `$PATH`.

### Store / rotate a credential
```sh
curl -X PUT $BASE/secrets/claude -H "$AUTH" -H "$JSON" -d '{
  "env_var": "ANTHROPIC_API_KEY",
  "value": "sk-ant-..."
}'
curl $BASE/secrets -H "$AUTH"               # metadata only, never plaintext
curl -X DELETE $BASE/secrets/claude -H "$AUTH"
```

### Live-test that an agent actually works
This launches the agent via hcom in print mode and reports whether it answered:
```sh
curl -X POST $BASE/agents/claude/test -H "$AUTH"   # → { ok, detail, reply }
```
Run this after storing a credential to confirm the key is valid before queuing real
work.

### Agent catalog (optional metadata)
`GET/POST/PATCH/DELETE /agent-catalog[/:id]` manages catalog entries (label,
env var, available models/efforts) used to populate UI pickers — not required to run
tasks.

---

## 4. hcom logs & status

hcom is the external engine that spawns and tracks agents. The scheduler drains its
event stream into a durable `hcom_log` table per `(run, task)`, so you read logs
over the API without touching hcom directly.

### Per-task agent log
```sh
curl "$BASE/tasks/auth/hcom?kind=message&limit=100&after=0"
```
Filters: `task`, `kind` (`message|status|life`), `after` (hcom id cursor), `limit`.

### Whole-run log (all agents in a workflow)
```sh
curl "$BASE/runs/ship-auth/hcom?limit=200"
```

### Deep transcript (shells out to `hcom transcript`)
```sh
curl $BASE/tasks/auth/transcript    # 404 if never claimed; 502 if hcom fails
```

### Direct hcom (host shell, for debugging the engine itself)
```sh
hcom status                         # is hcom installed/healthy
hcom list --json                    # live agents: name, status, tag (= task id)
hcom events --wait 5 --sql "id > 0" # tail raw events
```
The scheduler maps an agent back to its task via the `--tag <task-id>` it launched
with; `session` on a task is that agent's hcom name (the kill handle).

---

## 5. Operating the daemon (host shell)

These are not API calls — they bring the system up so the API exists.

```sh
make dev          # import seed workfile + boot daemon (bg) + Vite dashboard (:51840)
make dev-backend  # import + serve, no UI
make serve        # build + serve only → http://127.0.0.1:46787
make kill         # free port 46787 / reap orphaned lazybonesd
make install-hcom # ensure hcom is on PATH
```
Daemon log under `make dev`: `/tmp/lazybones-dev.log`. Runtime state lives in
`.lazy/` (db at `.lazy/db`, worktrees at `.lazy/wt`).

Always confirm liveness before driving the API:
```sh
curl $BASE/health    # 200 = process + store up; 503 = store unreachable
```

---

## Quick reference: end-to-end recipe

```sh
BASE=http://127.0.0.1:46787; AUTH="Authorization: Bearer lazybones-loop"; JSON="Content-Type: application/json"

# 0. daemon up + agent ready
curl $BASE/health
curl -X PUT $BASE/secrets/claude -H "$AUTH" -H "$JSON" -d '{"env_var":"ANTHROPIC_API_KEY","value":"sk-ant-..."}'
curl -X POST $BASE/agents/claude/test -H "$AUTH"

# 1. workflow + tasks
curl -X POST $BASE/workflows -H "$AUTH" -H "$JSON" -d '{"id":"wf","title":"WF","workspace":{"repo":"/abs/repo","base_branch":"main","tool":"claude"}}'
curl -X POST $BASE/workflows/wf/tasks -H "$AUTH" -H "$JSON" -d '{"id":"t1","title":"Do X","spec":"...","deps":[]}'
curl -X POST $BASE/workflows/wf/tasks -H "$AUTH" -H "$JSON" -d '{"id":"t2","title":"PR","from_template":"open-pr","deps":["t1"]}'

# 2. go (optionally arm auto-retry first so blocks self-heal up to the cap)
curl -X PUT $BASE/tasks/t1/auto-retry -H "$AUTH" -H "$JSON" -d '{"strategy":"quick","max_retries":2}'
curl -X POST $BASE/workflows/wf/start -H "$AUTH"

# 3. supervise
curl -N $BASE/stream &
curl $BASE/tasks/t1
curl "$BASE/runs/wf/hcom?limit=200"

# 4. a task blocked? revive it (guided retry keeps its worktree + folds in guidance)
curl -X POST $BASE/tasks/t1/retry -H "$AUTH" -H "$JSON" -d '{"strategy":"long_term"}'
# ...or workshop it conversationally (a chat on a blocked task revives it):
curl -X POST $BASE/tasks/t1/chat  -H "$AUTH" -H "$JSON" -d '{"text":"the gate fails on lint — fix that first"}'
```
