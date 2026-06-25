# MCP service for lazybones — design scope

Status: draft for review · Owner: TBD · Date: 2026-06-25
Companion docs: [`docs/managing-with-ai.md`](../managing-with-ai.md) (the REST
playbook this exposes as tools), [`docs/agent/lazybones-agent-scope.md`](../agent/lazybones-agent-scope.md)
(the in-app management agent — the *first consumer* of this surface),
[`docs/workflows-scope.md`](../workflows-scope.md),
[`docs/doc-writer/README.md`](../doc-writer/README.md),
[`docs/design/extension-system.md`](../design/extension-system.md).

## 1. Goal

Expose lazybones over the **Model Context Protocol** so that the **lazybones
agent** *and any external agent* (Claude Desktop, the `claude` CLI, Cursor, a
custom rmcp client, …) can drive lazybones through **typed MCP tools** instead of
hand-rolled `curl`/cheat-sheet calls. Three capability groups, matching the three
subsystems the request named:

1. **Orchestration** — create and manage **tasks, skills, templates, workflows**
   ([`docs/workflows-scope.md`](../workflows-scope.md), [`docs/managing-with-ai.md`](../managing-with-ai.md)).
2. **Documents** — author and publish **branded documents**
   ([`docs/doc-writer/README.md`](../doc-writer/README.md)).
3. **Extensions** — author and install **WASM extensions**
   ([`docs/design/extension-system.md`](../design/extension-system.md)).

Plus a **read/supervision** group (state, logs, history) every agent needs.

### The one design rule, inherited verbatim

> **Authoring is not running.** An MCP session may freely *create* tasks /
> workflows / templates / skills / documents / extensions. *Starting, stopping,
> retrying, deleting, installing-and-granting* are gated — exactly the split the
> management-agent scope (§9) and the engine's `started_at` guard already enforce.

The MCP surface is therefore **not a new capability plane**. It is a **second
front door onto the existing REST capabilities**, bound by the **same
`Capability` enum and `ScopedSession`** ([`crates/lazybones-auth/src/capability.rs`](../../crates/lazybones-auth/src/capability.rs)).
Its blast radius equals its token's grant — nothing new is reachable that a
bearer token couldn't already reach over HTTP.

## 2. Why rmcp, and where it runs

**SDK:** [`rmcp`](https://crates.io/crates/rmcp) — the official Rust MCP SDK
(`modelcontextprotocol/rust-sdk`). It fits this repo:

- **Server macros** (`#[tool_router]` / `#[tool]` / `#[tool_handler]`) turn a
  Rust impl block into a typed tool surface with JSON-Schema generated from
  `schemars`-derived argument structs — the same "typed, verb-per-file" ergonomic
  the codebase already favours, no hand-written schemas.
- **Axum-native HTTP transport** (`transport-streamable-http-server` /
  `transport-sse-server`) mounts as a normal axum sub-router — so MCP lives
  **inside the running `lazybonesd`**, sharing `AppState`/`StoreHandle` directly,
  with **no HTTP-to-self** (the same property the REST handlers rely on, see
  [`state.rs`](../../crates/lazybones-api/src/state.rs)).
- **`#![forbid(unsafe_code)]`-clean** — pure Rust, no native plugin/`dlopen`, so
  it respects the workspace safety posture (same constraint that picked Wasmtime
  for extensions).
- A **stdio** transport (`transport-io`) is available for a thin external bridge
  binary (§4.2).

### 2.1 Recommended topology

```
   external agent ──HTTP (streamable) + Bearer──┐
   (Claude Desktop, claude CLI,                  │
    Cursor, custom rmcp client)                  ▼
                            ┌───────────────────────────────────────────┐
   lazybones agent ───────► │ lazybonesd (one process, axum)             │
   (in-app management        │   REST routes        (existing)            │
    runner, §5)              │   /mcp  ── rmcp StreamableHttpService ──┐  │
                            │                                         │  │
                            │   crate: lazybones-mcp                  │  │
                            │     ToolRouter over AppState            │  │
                            │     (StoreHandle + engine handles)      │  │
                            │     capability check per tool ──────────┘  │
                            └───────────────────────────────────────────┘
                                         shared StoreHandle / Hcom / Registry
```

**One tool implementation, mounted in-process at `GET/POST /mcp`.** Both the
in-app lazybones agent and external agents speak to the *same* `/mcp` endpoint
over the streamable-HTTP transport, authenticated with a **scoped bearer token**
(§3) — precisely how the management runner already injects `LAZYBONES_TOKEN` +
`LAZYBONES_BASE_URL` into its agent ([`management/runner.rs`](../../crates/lazybones-engine/src/management/runner.rs)).
The tools call `StoreHandle` and the engine handles **directly**, reusing every
existing store verb and guard — they are a typed *mirror* of the REST routes, not
a reimplementation of the domain logic.

### 2.2 Why not embed the store in a standalone stdio binary

A separate process cannot share the daemon's embedded SurrealDB handle (single
writer, file-backed). Embedding a *second* store handle would fork the source of
truth — the exact anti-pattern SCOPE.md principle 6 forbids. So the MCP server
**is part of `lazybonesd`** and the only out-of-process option is a **thin stdio↔HTTP
bridge** (§4.2) that forwards to `/mcp` — it holds no state.

## 3. Authentication & the capability mapping (the safety model)

An MCP connection is authenticated exactly like a REST request: a **bearer token
resolved to a `ScopedSession`** via `AppState::session_for`
([`state.rs`](../../crates/lazybones-api/src/state.rs)). The token arrives in the
HTTP `Authorization` header of the streamable-HTTP transport. **Every tool
re-checks the session's capability before acting** — the same `session.can(cap)`
gate the routes use. No token ⇒ only the unguarded **read** tools resolve
(mirroring "GET reads are open" in [`managing-with-ai.md`](../managing-with-ai.md)).

| Token kind | Source | Grant | MCP tools it can call |
|---|---|---|---|
| **loop token** | `LAZYBONES_LOOP_TOKEN`, seeded at boot | `loop_grant()` (all) | everything, incl. lifecycle/`Extension`/`Secret`-adjacent |
| **management token (Author)** | `mint_management_token` (default) | `Read, Author, Document` | author tasks/workflows/templates/skills, documents, **not** start/delete/install |
| **management token (AuthorAndManage)** | minted on request | `+ Block` | + *propose* lifecycle; still no `Claim`/`Secret`/`Extension` |
| **read-only token** | minted | `Read` | supervision/read tools only |

This table is the whole authorization story — it is `Capability` reused 1:1.
Mapping consequences, made explicit:

- **`start`/`stop`/`restart`/`delete` workflow, `retry`/`cancel`/`auto-retry`
  task** → require `Block`/`Claim`-class grants the **Author** profile lacks. An
  external agent on a default management token therefore **cannot start a run**,
  matching the management-agent §9/§10 rule. To let an agent start things, the
  operator mints an `AuthorAndManage` or loop token *deliberately* — it is never
  the default.
- **`install extension` / `set grants` / `enable`** → require `Capability::Extension`,
  which **no management profile holds** ([`capability.rs`](../../crates/lazybones-auth/src/capability.rs)
  is explicit: "loop only"). So an MCP agent can **author** an extension's source
  and manifest (a Document/file-writing act) but **installing + granting host
  capabilities to sandboxed WASM is loop-only** — the extension-system §3.3 trust
  boundary holds through MCP unchanged.
- **`secrets`** → `Capability::Secret`, loop-only, **never exposed as an MCP
  tool** at all (not even read) — credentials never traverse the agent surface.

> Net: the MCP server adds **zero** new privilege. The hard guarantees
> (`started_at`, loop-only `Extension`/`Secret`) are enforced *below* the tool
> layer, so a confused or hostile MCP client is bounded by its token exactly like
> a confused REST client.

## 4. Transports

### 4.1 In-process streamable-HTTP (primary, v1)

Mount rmcp's `StreamableHttpService` as an axum sub-router at `/mcp` in
[`routes/mod.rs`](../../crates/lazybones-api/src/routes/mod.rs), behind the same
`cors_layer()` and body limit. Session state is held by rmcp's
`LocalSessionManager`. This is what the in-app lazybones agent uses (localhost),
and what HTTP-capable external clients register against:

```jsonc
// an external client's MCP config
{ "lazybones": {
    "url": "http://127.0.0.1:46787/mcp",
    "headers": { "Authorization": "Bearer <minted-token>" } } }
```

### 4.2 stdio bridge (P4, for stdio-only clients)

Some clients (older Claude Desktop configs) speak **stdio** only. Ship a tiny
`lazybones-mcp` **bin** that is a stateless rmcp **stdio server whose tool calls
proxy to `/mcp`** over HTTP with a token from `LAZYBONES_TOKEN`. It holds no
store, no domain logic — pure transport adaptation — so the tool surface is
defined once (§5) and reused. Registered via `claude mcp add lazybones -- lazybones-mcp`.

## 5. Crate & code layout

New crate **`crates/lazybones-mcp`** (edition 2024, Rust 1.93, inherits workspace
deps like [`lazybones-render`](../../crates/lazybones-render/Cargo.toml)), added
to the root `Cargo.toml` `members`. It depends on `lazybones-store`,
`lazybones-engine`, `lazybones-auth`, `lazybones-ext`, and `rmcp`. `lazybones-api`
depends on it to mount the route; `lazybones-cli`/`serve.rs` wires nothing new
(the route is part of `router()`).

```
crates/lazybones-mcp/
  Cargo.toml
  src/
    lib.rs            # ToolRouter assembly + `mcp_router(AppState) -> axum::Router`
    server.rs         # the #[tool_handler] struct holding AppState; ServerHandler impl (name, version, instructions)
    auth.rs           # Authorization header → ScopedSession; per-tool capability guard helper
    error.rs          # McpError mapping (store/engine/auth errors → MCP error payloads), mirrors api/error.rs
    args.rs           # schemars-derived argument structs (shared input DTOs)
    tools/
      mod.rs          # barrel
      orchestrate.rs  # tasks/skills/templates/workflows tools (§6.1)
      documents.rs    # document/asset/branding tools (§6.2)
      extensions.rs   # extension authoring + (loop-only) install tools (§6.3)
      supervise.rs    # read/state/log/history tools (§6.4)
  tests/
    capability_test.rs   # each tool refuses without its capability
    orchestrate_test.rs  # author-a-workflow round-trip over the in-process router
```

Each tool is a thin method: deserialize typed args → `auth.require(session, cap)`
→ call the existing `StoreHandle`/engine verb → serialize the existing domain
type as the tool result. **No business logic lives here** — it is the REST
handlers' twin, sharing the same store boundary, so the two surfaces can never
drift in behaviour.

## 6. The tool surface

Tools are named `<group>.<verb>` so a client lists them grouped. Reads need no
capability; every mutator names the capability it checks. The set is a curated
**subset** of REST — the high-value authoring/supervision verbs, not a 1:1 dump.

### 6.1 Orchestration (`Author`; lifecycle gated as noted)

| Tool | REST twin | Capability |
|---|---|---|
| `workflow.create` | `POST /workflows` | `Author` |
| `workflow.add_task` | `POST /workflows/:id/tasks` (inline or `from_template`) | `Author` |
| `workflow.list` / `workflow.get` | `GET /workflows[/:id]` | read |
| `workflow.start` | `POST /workflows/:id/start` | **`Claim`** (loop/elevated only — the create≠run line) |
| `workflow.stop` / `resume` / `restart` | `POST /workflows/:id/{stop,resume,restart}` | **`Block`** (managed only) |
| `task.create` / `task.update` | `POST /tasks`, `PATCH /tasks/:id` | `Author` |
| `task.list` / `task.get` | `GET /tasks[/:id]` | read |
| `task.retry` / `task.auto_retry` / `task.cancel` | `POST /tasks/:id/{retry,cancel}`, `PUT …/auto-retry` | **`Block`** (managed only) |
| `template.create` / `template.list` / `template.get` / `template.delete` | `/templates*` | `Author` (delete gated) |
| `skill.create` / `skill.update` / `skill.list` / `skill.get` | `/skills*` | `Author` |
| `follow_up.file` | `POST /follow-ups` | read — the agent's escape hatch ("needs a human") |

> The default management token holds only `Read + Author + Document`, so the
> gated rows above **resolve as 403** for it — the agent authors, then tells the
> operator to press Start. Identical behaviour to the in-app agent's confirm-card
> flow, minus the UI (an MCP client has no confirm card, so the *capability* is
> the gate, full stop).

### 6.2 Documents (`Document`)

| Tool | REST twin |
|---|---|
| `document.create` / `update` / `get` / `list` | `/documents*` |
| `document.add_page` / `update_page` / `list_pages` | `/documents/:id/pages*` |
| `document.attach_reference` / `list_references` | `/documents/:id/references*` |
| `document.add_source` / `list_sources` | `/documents/:id/sources*` |
| `document.render` | `GET /documents/:id/render` (HTML preview as text) |
| `branding.create` / `update` / `list` | `/branding*` |
| `asset.list` / `asset.get_meta` | `/assets*` (metadata; binary upload stays REST — MCP tool args are JSON, not raw bytes) |
| `document.set_repo` / `document.publish` | `PUT …/repo`, `POST …/publish` |

All mutators check `Capability::Document`, the same guard the routes use
([`document_*` routes](../../crates/lazybones-api/src/routes/)). **Asset *bytes*
are not an MCP concern** — uploads remain `POST /assets` (raw body); MCP exposes
only metadata + reference-by-id, and a tool returns the `/assets/:id` URL for the
agent to fetch/serve out of band.

### 6.3 Extensions (author = `Document`/file-write; install = loop-only `Extension`)

The split here is sharp and deliberate (extension-system §3.3):

| Tool | What it does | Capability |
|---|---|---|
| `extension.scaffold` | Generate a `cargo component` guest skeleton + `lazybones.ext.toml` manifest + a federated-remote skeleton into a repo/worktree (authoring source code — a *file-writing* act, typically via an authored task/document, **not** a privileged install) | `Author`/`Document` |
| `extension.list` / `extension.get` | `GET /extensions[/:id]` | read |
| `extension.install` | `POST /extensions` (upload/url) | **`Extension`** (loop-only) |
| `extension.set_grants` / `enable` / `disable` | `/extensions/:id/{grants,enable,disable}` | **`Extension`** (loop-only) |
| `extension.invoke` | `POST /extensions/:id/invoke` (test) | **`Extension`** (loop-only) |

So "an agent writes a WASM extension" decomposes into: **author the guest source
+ manifest** (allowed — it is just code in a repo, landed through the normal
task/gate pipeline), then **a human/loop installs it and grants capabilities**
(loop-only, because installing sandboxed code and handing it `secrets-read` +
`http-fetch` is the single most privileged act on the surface). MCP does **not**
let an agent self-install + self-grant — that would defeat the entire §3.3
default-deny trust boundary.

### 6.4 Supervision / read (no capability)

`state.health`, `state.engine`, `state.agents`, `run.history`
(`GET /runs/:id`), `run.follow_ups`, `task.hcom_log`, `task.transcript`,
`run.hcom_log`. These mirror the open REST reads so an external agent can answer
"what is the state of X?" without a token.

> **Live updates:** MCP has no first-class server-push for resource changes in
> v1. The existing SSE `GET /stream` stays the realtime channel; MCP supervision
> tools are request/response snapshots. (An MCP `resources/subscribe` mapping onto
> the live bus is an open question, §9.)

## 7. Discovery, prompts & instructions

- The rmcp `ServerHandler::get_info` advertises server name/version and an
  **instructions** string distilled from [`managing-with-ai.md`](../managing-with-ai.md)
  — the same content the management runner folds into its system prompt
  ([`management/prompt.rs`](../../crates/lazybones-engine/src/management/prompt.rs)),
  so MCP clients get the house rules (authoring≠running, `auto` permission mode is
  global, etc.) without a separate cheat-sheet.
- **Skills as MCP prompts (optional, P3):** the existing `skill` records
  (markdown runbooks) can be surfaced via MCP `prompts/list` + `prompts/get`, so a
  client can pull `lazybones-add-workflow` as a reusable prompt. This reuses the
  skill store with zero new model.

## 8. Non-goals (v1)

- **No new privilege / no new capability variant.** MCP is a transport over the
  existing `Capability` set. If a tool would need a grant no token can hold today,
  it is out of scope, not a reason to widen the enum.
- **No secret tools.** `Capability::Secret` is never exposed (read or write).
- **No agent self-install of extensions.** Install + grant stay loop-only.
- **No raw-byte transport over MCP.** Asset/source binaries stay on REST raw-body
  uploads; MCP carries JSON + ids/urls.
- **No second store.** The server is in-process; the only out-of-process artdefact
  is the stateless stdio bridge.
- **No bespoke auth.** Reuse `ScopedSession`/token registry; no OAuth/DCR in v1
  (localhost, single-user — mirrors the REST surface's posture).

## 9. Open questions

1. **Token minting UX.** How does an operator get an MCP token for an external
   client? Reuse `mint_management_token` behind a new `POST /mcp/token` (profile
   in body, `Author` capability to mint), surfaced in Settings next to the
   management-agent card? → *Recommend yes; profile-scoped, copy-to-clipboard,
   revocable.*
2. **Resource subscriptions.** Map MCP `resources/subscribe` onto the existing
   live bus (`stream.rs`) so clients get push, or leave realtime to SSE and keep
   MCP request/response? → *Defer to P3; SSE covers realtime today.*
3. **Lifecycle via MCP at all.** Should `workflow.start` even exist as a tool
   (gated by capability), or be omitted entirely so MCP is *authoring-only* by
   construction? → *Recommend: include it but gate on `Claim`; the capability is
   the guarantee, and omitting it just pushes operators to curl.*
4. **stdio bridge timing.** P4, or pull forward if a key target client is
   stdio-only (Claude Desktop)? Depends on which clients we commit to.
5. **Per-tool rate/auditing.** Every tool call already lands in the transition/
   hcom logs via the store verbs; do we also want an `mcp_call` audit row? →
   *Probably yes once external (non-loopback) clients are allowed.*
6. **Skill prompts vs tools.** Expose skills as MCP prompts (P3) — worth it, or
   does the instructions string suffice?

## 10. Phasing

- **P0 — Spike ✅ done:** `lazybones-mcp` crate with rmcp; **one** read tool
  (`state.health`) + **one** author tool (`workflow.create`) over the in-process
  `StreamableHttpService` mounted at `/mcp`; prove a `claude mcp add` HTTP client
  authenticates with a minted token, lists tools, and creates a workflow that
  shows up over REST. *Decision gate: rmcp axum integration + token auth clean?*
- **P1 — Orchestration MVP ✅ done:** the full §6.1 tool set + §6.4 reads +
  capability guard + `error.rs` mapping; `POST /mcp/token` minting + Settings
  affordance (OQ1). Hard rule enforced: default token cannot `start`.
- **P2 — Documents ✅ done:** §6.2 tools over `Capability::Document`.
- **P3 — Extensions + prompts ✅ done (install loop-only):** §6.3 (author-only for
  agents; install/grant/invoke loop-only). Skills-as-prompts (§7) deferred.
- **P4 — stdio bridge + breadth (deferred):** the stateless `lazybones-mcp` stdio
  bin (§4.2), resource subscriptions (OQ2), audit rows (OQ5).

---

## Appendix — Build it as a lazybones workflow (seed)

Per the doc-writer execution model and the workflow house rules
(`workflow-authoring-house-rules`): a **sequential chain of tasks sharing ONE
worktree and ONE branch** — each `depends_on` the previous, `worktree_mode: reuse`
off the first task, **not** per-task PRs; the whole feature lands as one
reviewable branch. **Do not auto-start it** — author, hand back, the operator
presses Start.

Workflow `mcp-service` on `workspace.repo = /home/user/code/rust/lazybones`,
`base_branch: master`, `tool: claude`, one shared worktree:

| Task id | depends_on | Spec summary | Gate |
|---|---|---|---|
| `mcp-crate` | — | Scaffold `crates/lazybones-mcp` (Cargo.toml + `lib.rs`/`server.rs`/`auth.rs`/`error.rs`/`args.rs`), add to workspace `members`, add `rmcp` (server + `transport-streamable-http-server` + `macros`) to workspace deps. Empty `ToolRouter` + `ServerHandler` (name/version/instructions). | build + clippy |
| `mcp-mount` | `mcp-crate` | Mount `StreamableHttpService` at `/mcp` in [`routes/mod.rs`](../../crates/lazybones-api/src/routes/mod.rs); `lazybones-api` depends on `lazybones-mcp`. Wire `Authorization → session_for` (`auth.rs`) + the per-tool `require(cap)` helper. | build + clippy |
| `mcp-spike` | `mcp-mount` | P0 tools: `state.health` (read) + `workflow.create` (`Author`). Integration test: in-process router, minted Author token, create a workflow, assert it exists via `StoreHandle`; assert no-token `workflow.create` → unauthorized. | `cargo test -p lazybones-mcp` |
| `mcp-orchestrate` | `mcp-spike` | §6.1 full set + §6.4 reads; lifecycle tools present but capability-gated (`start`→`Claim`, stop/resume/retry→`Block`). Tests: each gated tool 403s on an Author token; author round-trips green. | `cargo test --workspace` |
| `mcp-token` | `mcp-orchestrate` | `POST /mcp/token` (mint profile-scoped token, `Author` to mint) + Settings affordance (OQ1). | build + clippy + test |
| `mcp-documents` | `mcp-orchestrate` | §6.2 document/branding/asset-metadata tools over `Capability::Document`. | `cargo test --workspace` |
| `mcp-extensions` | `mcp-documents` | §6.3: `extension.scaffold` (author), read tools; install/grant/invoke wired but `Capability::Extension`-gated (loop-only). Test: agent-token install → 403. | `cargo test --workspace` |
| `mcp-docs` | `mcp-extensions` | Update [`docs/managing-with-ai.md`](../managing-with-ai.md) with an MCP section; `claude mcp add` recipe; mark phases done here. | build |

Verification (end of chain): `cargo build --workspace`; `cargo clippy --workspace
--all-targets -- -D warnings`; `cargo test --workspace`; manual `claude mcp add
lazybones --transport http http://127.0.0.1:46787/mcp` with a minted token →
list tools → create a workflow → confirm it over `GET /workflows` and that a
default-token `workflow.start` is refused.
