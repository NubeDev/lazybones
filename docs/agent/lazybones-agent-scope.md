# Lazybones Agent — Scope & Design

**Status:** Draft scope (investigation only — nothing built yet)
**Author:** scoping session, 2026-06-21
**Companion docs:** [`docs/managing-with-ai.md`](../managing-with-ai.md) (the operator playbook this agent automates)

> File note: the originating brief named both `docs/agent/lazybones-agent-scope.md`
> and `docs/lazybones-agent-scope.md`. This doc lives at the former. If you prefer the
> flat path, `git mv` it — there is no other reference to update.

---

## 1. Summary

A first-class **Lazybones Agent**: a conversational assistant, embedded in the web UI,
whose job is to help the operator *manage lazybones itself* — author workflows, tasks,
templates, and skills; explain current state; and supervise runs. It is **page-aware**
(knows which workflow/task/page you have open), **configurable in Settings** (which tool,
model, effort, permissions it runs with), and it acts **only through the same REST API a
human operator uses** ([`crates/lazybones-api/src/routes/`](../../crates/lazybones-api/src/routes/)).

The single most important design rule, learned from a just-fixed scheduler bug (§9):
**authoring is not running.** The agent may freely *create* workflows/tasks/templates/skills
(the `Author` capability plane), but it must never *start* them. Starting, stopping,
retrying, and deleting are gated behind explicit human confirmation.

---

## 2. Goals / Non-goals

### Goals
- A ChatGPT-style chat panel that mounts on any UI page and can answer "what is the state
  of X?" and "create a workflow that does Y."
- Drive the documented REST surface to **author** workflows, tasks, task-templates, and skills.
- Be **grounded** in the page the operator is looking at (current workflow/task IDs).
- Be **configurable** in Settings — tool (claude/codex/…), model, effort, permission profile —
  reusing the existing agent-catalog + secret machinery.
- Make the agent's *capabilities* a strict subset of the operator's REST capabilities, with
  destructive/lifecycle actions behind confirmation.
- Persist chat history so a conversation survives reload.

### Non-goals (explicitly out of scope)
- **A "supervisor agent"** that autonomously watches runs and self-heals (auto-retry loops,
  auto-merge decisions, escalation policy). That is a separate, larger effort. The split here
  mirrors the existing one: lazybones already has an *automatic* scheduler loop
  ([`scheduler/run.rs`](../../crates/lazybones-engine/src/scheduler/run.rs)) and *manual*
  operator controls. This agent is an **operator aide**, not a second scheduler.
- **Starting/running work unattended.** See §9.
- **New capabilities not already in the REST surface.** The agent is bounded by what
  [`docs/managing-with-ai.md`](../managing-with-ai.md) documents; it invents no privileged path.
- **Bypassing the gate/merge pipeline.** The agent authors; the engine runs and lands.
- Multi-user / multi-tenant chat, RBAC beyond the existing capability tokens.

---

## 3. Architecture

### 3.1 The three planes today

Lazybones is a single-process Tokio daemon (`lazybonesd`):

- **HTTP/API plane** — Axum router in [`crates/lazybones-api/src/routes/mod.rs`](../../crates/lazybones-api/src/routes/mod.rs),
  handlers share a `StoreHandle` directly (no HTTP-to-self).
- **Scheduler plane** — `tokio::spawn(lazybones_engine::run(...))` from
  [`crates/lazybones-cli/src/serve.rs`](../../crates/lazybones-cli/src/serve.rs); ticks every
  `tick_secs`, promotes/claims/spawns task agents via the **hcom** CLI
  ([`scheduler/tick.rs`](../../crates/lazybones-engine/src/scheduler/tick.rs),
  [`hcom/spawn.rs`](../../crates/lazybones-engine/src/hcom/spawn.rs)).
- **UI plane** — React app in [`ui/`](../../ui/), talks to the API via
  [`ui/src/lib/api/client.ts`](../../ui/src/lib/api/client.ts) and subscribes to a single
  SSE stream `GET /stream` ([`ui/src/lib/hooks/use-live-stream.ts`](../../ui/src/lib/hooks/use-live-stream.ts)).

### 3.2 Where the Lazybones Agent runs — recommendation

**Run it as a new in-daemon component (a dedicated API surface + a thin agent runner),
not as a scheduler task.** Two options were considered:

| | Option A: spawn as an hcom task agent | **Option B: in-daemon agent service (recommended)** |
|---|---|---|
| Mechanism | Create a one-task workflow, let the scheduler `claim → spawn` it via hcom | New `POST /agent/chat` endpoint + a per-conversation hcom session managed off the scheduler loop |
| Lifecycle | Bound to task promote/claim/gate/merge | Free-running, request/response; no worktree, no gate, no merge |
| Worktree | Provisions a git worktree it does not need | None — it calls the REST API, doesn't edit a repo |
| **Create≠run risk** | High — it *is* a run; trips the very guardrail in §9 | Low — it never creates a run for itself |
| Reuse | Reuses spawn/kill/threading | Reuses hcom `spawn`/`send`/`events_since` directly ([`hcom/mod.rs`](../../crates/lazybones-engine/src/hcom/mod.rs)) |

Option A forces the management agent through the task lifecycle (promotion, gating, merge)
that exists to land *code changes onto a branch* — none of which applies to an agent whose
output is REST calls and chat text. It would also need a worktree and would itself be a "run,"
colliding with §9. **Option B** keeps the agent as a plain conversational service.

**Concretely (all "to build"):**

- A new module `crates/lazybones-engine/src/management/` (sibling to `scheduler/`) exposing
  `async fn chat_turn(...)` that:
  1. Loads the agent config from Settings (§5).
  2. Builds a system prompt = agent persona + the skills (§6) + the page context (§7) +
     a compact REST cheat-sheet derived from [`docs/managing-with-ai.md`](../managing-with-ai.md).
  3. Spawns / resumes an hcom session for the configured tool using the existing
     [`Hcom::spawn`](../../crates/lazybones-engine/src/hcom/spawn.rs) /`send`/`events_since`
     wrapper (reuse, do not reinvent — same client the scheduler uses).
  4. Streams the agent's tokens/events back to the UI (§8).
  5. The agent performs its work by calling the lazybones REST API with a **scoped bearer
     token** minted for the management agent (§10), so it is bound by capabilities exactly
     like any operator.
- New API routes under `crates/lazybones-api/src/routes/agent_*.rs` (to build):
  - `POST /agent/chat` — submit a user turn (+ page context), returns a conversation id / streams.
  - `GET  /agent/chat/:conversation` — fetch persisted history.
  - `GET  /agent/conversations` — list conversations.
- Streaming reuses the existing SSE infrastructure ([`routes/stream.rs`](../../crates/lazybones-api/src/routes/stream.rs)),
  with a new `agent_token` / `agent_message` `LiveEvent` variant (to build), OR a dedicated
  per-conversation SSE endpoint. See §8 for the trade-off.

### 3.3 "Through the same API a human would"

The management agent is handed a bearer token and a base URL and calls `POST /workflows`,
`POST /workflows/:id/tasks`, `POST /skills`, etc. It has **no privileged back door** into the
store. This is the core safety property: its blast radius equals its token's capabilities (§10),
and every action it takes is auditable through the same transition log (`GET /runs/:id`) and
hcom log a human's actions produce.

---

## 4. The REST surface the agent drives

From [`docs/managing-with-ai.md`](../managing-with-ai.md) and the route handlers, classified by
plane. **The classification is the safety model** — see §10.

### Authoring plane (`Author` capability) — agent may do unattended
- `POST /workflows` — create a workflow (lands `active`, **no `started_at`** → cannot run; §9)
- `POST /workflows/:id/tasks` — add a task to a workflow
- `POST /tasks`, `PATCH /tasks/:id` — create / edit a standalone task
- `POST /templates`, `PUT /templates/:id` — create / edit a task-template
- `POST /skills`, `PUT /skills/:id` — create / edit a skill
- `POST /templates/:id/attachments`, `DELETE …` — attach/detach a skill to a template
- `POST /tasks/:id/ready` — promote a task to `ready` *(NB: still inert until the workflow is started — §9)*

### Lifecycle plane — **confirmation required** (these *run* or *destroy* work)
- `POST /workflows/:id/start` — **the Start button.** Stamps `started_at`; only after this do
  tasks actually get claimed and spawned ([`workflows_start.rs`](../../crates/lazybones-api/src/routes/workflows_start.rs)).
- `POST /workflows/:id/stop`, `/stop-reset`, `/resume`, `/restart`
- `POST /tasks/:id/cancel`, `/retry`, `/block`; `PUT /tasks/:id/auto-retry`
- `DELETE /tasks/:id`, `DELETE /workflows/:id`, `DELETE /templates/:id`, `DELETE /skills/:id`

### Supervision / read plane (no auth) — agent may read freely
- `GET /workflows`, `/workflows/:id`, `/workflows/:id/tasks`
- `GET /tasks`, `/tasks/:id`, `/tasks/:id/chat`, `/tasks/:id/hcom`, `/tasks/:id/transcript`
- `GET /runs/:id` (transition history), `/runs/:id/hcom`, `/runs/:id/follow-ups`
- `GET /templates`, `/skills`, `/agent-catalog`, `/stream`
- `POST /follow-ups` (`Block`) — file a blocker/note for the human; **the agent's escape hatch**
  instead of taking a lifecycle action it isn't allowed to.

> Scheduler-only endpoints (`POST /tasks/:id/claim`, `/gate`, `/done`, `/tasks/promote`) require
> the `Claim` capability and are **never** in the agent's grant. The agent authors; the scheduler
> claims and runs.

---

## 5. Settings model

### 5.1 What exists to reuse
- **Agent catalog** ([`crates/lazybones-store/src/agent/model.rs`](../../crates/lazybones-store/src/agent/model.rs)):
  `AgentCatalog { id, label, env_var, models[], default_model, efforts[], default_effort, … }`,
  seeded from `agents.default.yaml` (claude, codex, gemini, …). Exposed at `GET /agent-catalog`.
  This already enumerates **which tools exist and which models/efforts each supports** — the
  exact menus the Settings UI needs.
- **Secrets** ([`crates/lazybones-store/src/secret/`](../../crates/lazybones-store/src/secret/)):
  per-tool API keys, AES-256-GCM at rest, `PUT /secrets/:tool`, metadata via `GET /secrets`.
  The management agent's chosen tool authenticates with the **same stored credential** task
  agents use — no new secret needed.
- **Per-tool permission flags** ([`crates/lazybones-engine/src/config.rs`](../../crates/lazybones-engine/src/config.rs)):
  `EngineConfig.permission_flags: HashMap<String, Vec<String>>` (e.g. claude →
  `["--dangerously-skip-permissions"]`). Pattern to reuse for the agent's CLI permission flags.
- **Settings UI** ([`ui/src/features/settings/settings-page.tsx`](../../ui/src/features/settings/settings-page.tsx)):
  today holds the daemon-connection block + the `AgentsPanel`
  ([`ui/src/features/agents/agents-panel.tsx`](../../ui/src/features/agents/agents-panel.tsx)).
  Add a new card here.

### 5.2 New config — `ManagementAgentConfig` (to build)

A **single global record** (one management agent for the install, MVP). Store it as a new
SCHEMALESS row `settings:management_agent`, mirroring how `agent`/`skill` rows are persisted
([`crates/lazybones-store/src/agent/row.rs`](../../crates/lazybones-store/src/agent/row.rs)):

```rust
// crates/lazybones-store/src/management_agent/model.rs  (to build)
pub struct ManagementAgentConfig {
    pub tool: String,                 // FK into agent catalog, e.g. "claude"
    pub model: Option<String>,        // validated ⊆ catalog.models for `tool`
    pub effort: Option<String>,       // validated ⊆ catalog.efforts for `tool`
    pub permission_profile: PermissionProfile,  // see §10
    pub enabled_skills: Vec<String>,  // skill ids the agent may use as tools (§6)
    pub permission_flags: Vec<String>,// extra CLI flags for the tool process
    pub updated_at: String,
}

pub enum PermissionProfile {
    ReadOnly,        // GET-only: explain state, never mutate
    Author,          // + create/edit workflows/tasks/templates/skills (default)
    AuthorAndManage, // + lifecycle (start/stop/retry/delete) — still confirm in UI (§10)
}
```

- **Store:** new module `crates/lazybones-store/src/management_agent/` with
  `get_management_agent()` / `put_management_agent()` on `StoreHandle`
  ([`handle.rs`](../../crates/lazybones-store/src/handle.rs)).
- **API:** `GET /settings/management-agent`, `PUT /settings/management-agent`
  (`Author` capability), validating `model`/`effort` against `GET /agent-catalog`.
- **UI:** new "Lazybones Agent" card in the settings page: tool `<Select>` (from
  `useAgentCatalog()` [`ui/src/lib/hooks/use-agent-catalog.ts`](../../ui/src/lib/hooks/use-agent-catalog.ts)),
  dependent model/effort selects, a permission-profile radio, and a skills multi-select.
  New hook `useManagementAgentConfig()` + `useUpdateManagementAgentConfig()`.

---

## 6. Skill design

### 6.1 What a skill is today
[`crates/lazybones-store/src/skill/model.rs`](../../crates/lazybones-store/src/skill/model.rs):
`Skill { id, title, description, body, created_at, updated_at }` — **provider-agnostic markdown
instructions**, no runtime, no state. CRUD at `/skills` (read open; create/update/delete `Author`).
Seeded from [`skills.default.yaml`](../../crates/lazybones-store/src/skill/skills.default.yaml)
(`code-review-rust`, `write-tests`, `open-pr`, `conventional-commits`). Skills can be **attached**
to templates via the polymorphic `attachment` table
([`crates/lazybones-store/src/attachment/`](../../crates/lazybones-store/src/attachment/)).

**Key reuse decision:** skills are just markdown today; they are *advisory text*, not executable
tools. The management agent turns the relevant skills into **its operating procedures**: each
"action" skill is a markdown runbook describing *which REST calls to make, in what order, with
what guardrails*. The agent reads them as part of its system prompt and executes the REST calls
itself (it already has HTTP + the bearer token). This needs **no new skill runtime** — it reuses
the existing store, CRUD, and the `enabled_skills` selector from §5.

> Trade-off worth flagging as an open question (§11): pure-markdown skills are flexible but
> unstructured. If we later want *deterministic* skill execution (validated inputs, typed
> outputs), we'd add an optional structured `action` block to the skill model. MVP keeps them
> markdown runbooks.

### 6.2 New skills to seed (to build, added to `skills.default.yaml`)
| Skill id | Purpose | Primary REST calls | Plane |
|---|---|---|---|
| `lazybones-add-workflow` | Author a new workflow (no start) | `POST /workflows` | Author |
| `lazybones-add-task` | Add a task to a workflow | `POST /workflows/:id/tasks` (or `POST /tasks`) | Author |
| `lazybones-add-template` | Author a reusable task-template | `POST /templates`, optional `POST /templates/:id/attachments` | Author |
| `lazybones-add-skill` | Author a new skill | `POST /skills` | Author |
| `lazybones-supervise` | Summarize run/task state, surface blockers | `GET /workflows/:id`, `/tasks`, `/runs/:id/follow-ups` | Read |
| `lazybones-retry` (gated) | Propose a retry; **requires confirmation** | `POST /tasks/:id/retry` | Lifecycle → confirm |

### 6.3 Worked example — `lazybones-add-workflow`

**Skill body (markdown, stored in the `skill` table):**

```markdown
# Skill: Add a workflow

Author a new lazybones workflow from a natural-language description. You AUTHOR ONLY —
you never start it. After creating, tell the operator to press Start (or, if your
permission profile allows and they confirm, call start as a separate, explicit step).

## Inputs to gather (ask if missing)
- A unique `id` (kebab-case) and human `title`.
- `workspace.repo` — absolute path. If the operator is on a workflow page, default to that
  workflow's repo (see page context `workflow.repo`).
- Optional: base_branch, tool/model/effort (else inherit engine defaults).

## Procedure
1. Confirm no id clash: `GET /workflows/:id` → expect 404.
2. Create the workflow:
   POST /workflows
   { "id", "title", "workspace": { "repo", "base_branch?", "tool?", "model?", "effort?", "merge?" } }
3. For each task the operator described, run the `lazybones-add-task` skill.
4. Report the created workflow and its task list. Do NOT call /start.
   End with: "Created `<id>` with N tasks. Press Start when you're ready to run it."

## Guardrails
- Creating a workflow returns lifecycle `active` but NO `started_at` — it will NOT run until
  an operator starts it. This is intentional; never call POST /workflows/:id/start yourself
  without an explicit confirmation step.
```

**End-to-end flow:**
1. Operator (on the Workflows page) types: *"make a workflow in /repo/foo that adds a healthcheck
   endpoint and a test for it."*
2. UI sends the turn + page context `{ page: "workflows" }` to `POST /agent/chat`.
3. Management runner builds the system prompt: persona + `lazybones-add-workflow` +
   `lazybones-add-task` bodies + REST cheat-sheet + page context.
4. Agent calls `GET /workflows/add-healthcheck` → 404 (id free), then
   `POST /workflows {id:"add-healthcheck", title:…, workspace:{repo:"/repo/foo"}}` →
   `200 Run{ lifecycle: active, started_at: null }`.
5. Agent calls `POST /workflows/add-healthcheck/tasks` twice (impl task, then a test task with
   `deps:["impl"]`).
6. Agent streams: *"Created `add-healthcheck` with 2 tasks (impl → test). It's not running yet —
   press **Start** to launch it."* The UI's transition stream already reflects the new workflow.
7. Operator clicks the existing **Start** control
   ([`ui/src/features/workflows/workflow-controls.tsx`](../../ui/src/features/workflows/workflow-controls.tsx)).
   Nothing ran until that human click — §9 holds.

---

## 7. Page-context protocol

### 7.1 What each page can supply
Navigation is **view-state, not URL** ([`ui/src/app/navigation.ts`](../../ui/src/app/navigation.ts),
[`ui/src/app/router.tsx`](../../ui/src/app/router.tsx)): `View = "dashboard" | "templates" |
"skills" | "workflows" | "tasks" | "runs" | "settings"`, with detail panels opened by `id` prop
rather than route params. So context must be **lifted from component state**, not parsed from a URL.

| Page / panel | IDs in scope | Source |
|---|---|---|
| Workflow detail | `workflow_id`, `repo`, `base_branch`, `worktree_mode`, `task_ids[]`, progress | [`workflow-detail.tsx`](../../ui/src/features/workflows/workflow-detail.tsx) |
| Task detail / page | `task_id`, `run_id`, `status`, `spec`, deps/owns | [`task-detail.tsx`](../../ui/src/features/tasks/detail/task-detail.tsx) |
| Templates / Skills | selected `template_id` / `skill_id` (if any) | respective feature dirs |
| Dashboard / Runs / Settings | none (global mode) | — |

### 7.2 The protocol (to build)
A small typed envelope sent with every chat turn:

```ts
// ui/src/types/agent.ts  (to build)
interface PageContext {
  view: View;                      // which page
  workflow_id?: string;
  task_id?: string;
  run_id?: string;
  selected_template_id?: string;
  selected_skill_id?: string;
}
```

- Provide a React context `AgentContextProvider` mounted in the app shell; each detail panel
  calls `useSetAgentContext({ workflow_id, … })` on mount / selection change (the panels already
  hold these IDs in scope). The chat panel reads it and attaches it to `POST /agent/chat`.
- Server-side, the runner renders the context into the system prompt as ground truth
  ("The operator is currently viewing workflow `X` in repo `Y`."), and the agent prefers those
  IDs as defaults. It still re-reads authoritative state via `GET` before acting (IDs are hints,
  not trusted state — same discipline as §9's two-stage checks).

---

## 8. Chat UI

### 8.1 What exists to copy
There is **no global agent chat** today. There *is* a task-scoped chat
([`ui/src/features/tasks/detail/task-chat.tsx`](../../ui/src/features/tasks/detail/task-chat.tsx))
with: message bubbles (user right/primary, agent left/muted), a textarea composer (Enter sends,
Shift+Enter newline), delivery-status hints, and React Query hooks
([`use-chat.ts`](../../ui/src/lib/hooks/use-chat.ts)). **Reuse its bubble + composer presentation;
do not reuse its task-bound data layer.**

### 8.2 Component shape (to build)
- `ui/src/features/agent/agent-panel.tsx` — a slide-over / docked right panel, mountable from the
  app shell so it overlays any page. A persistent launcher button in the top bar.
- `agent-message-list.tsx` (bubbles, reused styling), `agent-composer.tsx`,
  `agent-confirm-card.tsx` (renders a "the agent wants to **Start** workflow X — Confirm/Cancel"
  card for gated actions, §10).
- Hooks: `useAgentChat(conversationId)`, `usePostAgentTurn()`, `useAgentStream(conversationId)`.

### 8.3 Message persistence
Mirror the task chat-message model ([`crates/lazybones-store/src/`](../../crates/lazybones-store/src/) chat module):
a new `agent_message { conversation_id, role: "user"|"agent"|"tool", text, at }` table (to build),
plus an `agent_conversation { id, page_context_snapshot, created_at }` table. History via
`GET /agent/chat/:conversation`. This makes conversations durable across reload and auditable.

### 8.4 Streaming
Two viable routes; **recommend a dedicated per-conversation SSE endpoint**
`GET /agent/chat/:conversation/stream` so token streams don't fan out to every connected client
on the shared global `/stream` (the global stream is for transitions/activity/hcom/chat events
seen by all viewers — [`use-live-stream.ts`](../../ui/src/lib/hooks/use-live-stream.ts)). The
runner drives it from hcom's `events_since` drain
([`hcom/events.rs`](../../crates/lazybones-engine/src/hcom/events.rs)) — the same mechanism
`hcom_tail` uses to stream task-agent output. Emit `token`, `tool_call` (a REST action the agent
took, for transparency), `confirm_request` (a gated action awaiting the human), and `done` events.

---

## 9. Prior art / guardrails — create ≠ run

### 9.1 The bug we just fixed
Creating an `active` workflow auto-promoted and **ran** its root task even though no operator had
pressed Start. Root cause: the promote query excluded only `Stopped` runs, and the claim guard
checked only `lifecycle == Active`, **ignoring `started_at`**. A freshly created workflow is
`active` with `started_at = None`, so it slipped through and the scheduler spawned an agent.

**Fix (commit `c35443a`):**
- [`crates/lazybones-store/src/handle.rs`](../../crates/lazybones-store/src/handle.rs) —
  `unpromotable_run_ids()` now excludes runs that are `Stopped` **or** `started_at.is_none()`.
- [`crates/lazybones-engine/src/scheduler/tick.rs`](../../crates/lazybones-engine/src/scheduler/tick.rs) —
  `promote()` excludes those runs; the claim/spawn guard now `continue`s when
  `r.lifecycle != Active || r.started_at.is_none()`.
- Net: **a workflow runs only after `POST /workflows/:id/start` stamps `started_at`.** Two-stage
  enforcement (promote *and* claim) so a single missed check can't leak.

### 9.2 Why it matters here, and the rule
The management agent **creates workflows and tasks programmatically** — exactly the path that bug
lived on. The scope-level rule:

> **The agent authors; the human starts.** Creating a workflow/task is safe and unattended.
> Starting it (`POST /workflows/:id/start`) is a lifecycle action that requires explicit human
> confirmation in the UI (§10). The agent's default permission profile (`Author`) cannot reach
> `/start` at all. Even `AuthorAndManage` renders a confirm card, never a silent start.

This is belt-and-suspenders with the engine fix: even if the agent (or a future bug) *tried* to
start something, an unstarted workflow won't run, and the agent's token won't carry `Claim`. The
guardrail is enforced at three layers: the agent's permission profile, the confirm-in-UI step,
and the engine's `started_at` guard.

---

## 10. Permissions & safety

### 10.1 Capability model to reuse
[`crates/lazybones-auth/src/capability.rs`](../../crates/lazybones-auth/src/capability.rs):
`Capability::{ Author, Block, Claim, Secret, Read, … }`. The management agent gets a **scoped
bearer token** minted at chat-session start whose capabilities are derived from the configured
`PermissionProfile` (§5):

| Profile | Token capabilities | Can do | Cannot do |
|---|---|---|---|
| `ReadOnly` | `Read` | explain state, summarize, draft specs in chat | any mutation |
| `Author` *(default)* | `Read`, `Author` | create/edit workflows, tasks, templates, skills, attachments | start/stop/retry/delete, secrets, claim |
| `AuthorAndManage` | `Read`, `Author`, `Block` | + propose lifecycle actions (each still confirmed in UI) | `Claim` (scheduler-only), `Secret` |

`Claim` and `Secret` are **never** granted to the management agent. It manages workflows; it does
not run the scheduler loop or read decrypted secrets.

### 10.2 Unattended vs. confirmation-required

**Unattended (agent acts, then reports):**
- All `Author`-plane authoring: `POST /workflows`, `/workflows/:id/tasks`, `/tasks`, `/templates`,
  `/skills`, attachments; `PATCH`/`PUT` edits.
- All reads.
- `POST /follow-ups` — filing a note/blocker for the operator.

**Confirmation-required (agent proposes a `confirm_request`; human clicks Confirm):**
- `POST /workflows/:id/start` **(the create≠run line)**, `/stop`, `/stop-reset`, `/resume`, `/restart`
- `POST /tasks/:id/retry`, `/cancel`, `/block`; `PUT /tasks/:id/auto-retry`
- **All deletes:** `DELETE /tasks/:id`, `/workflows/:id`, `/templates/:id`, `/skills/:id`

Mechanism: when the agent wants a gated action, it does **not** call the endpoint. It emits a
`confirm_request` event describing the action + exact REST call. The UI renders an
`agent-confirm-card`. On Confirm, the **UI** (with the operator's own loop token) issues the call —
so the destructive/lifecycle action is always taken under human authority, never the agent's token.
This also means `AuthorAndManage`'s `Block` capability is a convenience for *reading* blocker state,
not a license to silently retry.

### 10.3 Other safety notes
- **Idempotency / id cl-clash:** authoring endpoints 409 on duplicate id; the skills tell the agent
  to `GET` first and to surface 409s rather than mutating blindly.
- **Auditability:** every agent action flows through the same transition log (`GET /runs/:id`),
  hcom log, and (for its own turns) the `agent_message` table. Nothing is invisible.
- **Blast radius = token capabilities.** Because it goes through REST, the worst a compromised or
  confused agent can do is bounded by §10.1 — and it cannot start or run anything.

---

## 11. Open questions
1. **One agent or many?** MVP = one global `ManagementAgentConfig`. Do we ever want per-workflow
   or per-repo management agents? (Schema is a single row today; a table keyed by scope later.)
2. **Skill structure.** Keep skills pure markdown runbooks (flexible, MVP) or add an optional typed
   `action` block for deterministic execution + validation? (§6.1)
3. **Tool/function-calling vs. prompt-and-parse.** Do we expose the REST surface to the agent as
   structured tool definitions (cleaner, tool-specific) or as a markdown cheat-sheet it calls via
   shell/HTTP (tool-agnostic, works for codex/gemini/…)? Leaning tool-agnostic to match the
   multi-tool catalog, but claude-specific tool-calling would be more reliable.
4. **Streaming transport.** Dedicated per-conversation SSE (recommended) vs. extending the global
   `/stream` with conversation-scoped filtering.
5. **Confirm-card authority.** Confirmed actions issued by the UI with the operator's loop token
   (recommended) vs. minting a short-lived elevated agent token per confirmation.
6. **Context privacy.** Page context may include repo paths; fine for a local single-user daemon,
   but worth a note if lazybones ever goes multi-user.
7. **Cost / session lifecycle.** When does an hcom management session get killed — per turn, per
   conversation, idle-timeout? Reuse `kill_tag` discipline from the scheduler.

---

## 12. Phased build plan

### Phase 0 — Spike (no UI)
- `crates/lazybones-engine/src/management/` runner that takes a hard-coded tool + a one-shot prompt,
  spawns an hcom session, and lets the agent hit a read-only REST token. Prove the loop:
  prompt → hcom → REST `GET` → streamed answer. No persistence.

### Phase 1 — MVP (author + read, single agent)
- `ManagementAgentConfig` store module + `GET/PUT /settings/management-agent` + Settings card.
- `POST /agent/chat`, `GET /agent/chat/:conversation`, conversation + message persistence tables.
- Scoped token minting for `ReadOnly` / `Author` profiles.
- Seed skills: `lazybones-add-workflow`, `lazybones-add-task`, `lazybones-add-template`,
  `lazybones-add-skill`, `lazybones-supervise`.
- Page-context envelope + `AgentContextProvider`; wire workflow + task detail pages.
- Chat panel (reusing task-chat bubble/composer styling) with per-conversation SSE streaming.
- **Hard rule enforced:** agent can author but the panel has no path to `/start`.

### Phase 2 — Managed actions (gated lifecycle)
- `AuthorAndManage` profile; `confirm_request` events + `agent-confirm-card`; UI-issued confirmed
  calls for start/stop/retry/cancel/delete.
- `lazybones-retry` skill; richer supervision (summarize follow-ups, explain blocked tasks).

### Phase 3 — Polish & breadth
- Conversation list/history UI; mount agent context on templates/skills/runs pages.
- Optional structured skill actions (resolve open question 2).
- Multi-agent / per-scope config if warranted (open question 1).

### Explicitly deferred (separate effort)
- An autonomous **supervisor** that watches and self-heals runs (non-goal, §2).

---

## Appendix A — Key files cited

**API routes:** [`routes/mod.rs`](../../crates/lazybones-api/src/routes/mod.rs),
`workflows_create.rs`, `workflows_start.rs`, `workflows_add_task.rs`, `skills_*.rs`,
`templates_*.rs`, `template_attachments.rs`, `follow_ups.rs`, `agent_catalog.rs`, `secrets_*.rs`,
`stream.rs`, `chat.rs` (all under [`crates/lazybones-api/src/routes/`](../../crates/lazybones-api/src/routes/)).
DTOs: [`dto.rs`](../../crates/lazybones-api/src/dto.rs).

**Store:** [`skill/`](../../crates/lazybones-store/src/skill/),
[`agent/`](../../crates/lazybones-store/src/agent/),
[`secret/`](../../crates/lazybones-store/src/secret/),
[`attachment/`](../../crates/lazybones-store/src/attachment/),
[`handle.rs`](../../crates/lazybones-store/src/handle.rs),
[`init_schema.rs`](../../crates/lazybones-store/src/init_schema.rs).

**Engine:** [`scheduler/tick.rs`](../../crates/lazybones-engine/src/scheduler/tick.rs),
[`scheduler/run.rs`](../../crates/lazybones-engine/src/scheduler/run.rs),
[`hcom/spawn.rs`](../../crates/lazybones-engine/src/hcom/spawn.rs),
[`hcom/events.rs`](../../crates/lazybones-engine/src/hcom/events.rs),
[`config.rs`](../../crates/lazybones-engine/src/config.rs),
[`serve.rs`](../../crates/lazybones-cli/src/serve.rs).

**Auth:** [`capability.rs`](../../crates/lazybones-auth/src/capability.rs).

**UI:** [`settings-page.tsx`](../../ui/src/features/settings/settings-page.tsx),
[`agents-panel.tsx`](../../ui/src/features/agents/agents-panel.tsx),
[`workflow-detail.tsx`](../../ui/src/features/workflows/workflow-detail.tsx),
[`workflow-controls.tsx`](../../ui/src/features/workflows/workflow-controls.tsx),
[`task-detail.tsx`](../../ui/src/features/tasks/detail/task-detail.tsx),
[`task-chat.tsx`](../../ui/src/features/tasks/detail/task-chat.tsx),
[`use-live-stream.ts`](../../ui/src/lib/hooks/use-live-stream.ts),
[`use-agent-catalog.ts`](../../ui/src/lib/hooks/use-agent-catalog.ts),
[`navigation.ts`](../../ui/src/app/navigation.ts), [`router.tsx`](../../ui/src/app/router.tsx).
