# Extension System — Design Scope

Status: draft for review · Owner: TBD · Date: 2026-06-24

## 1. Goal

Let third parties (and us) extend lazybones **without forking or recompiling the core**, across two planes:

- **Backend plane** — new capabilities that run inside/alongside the daemon: custom gate checks, merge strategies, agent providers, blob/store backends, event reactions, new domain logic, scheduled jobs.
- **Frontend plane** — new UI surfaces (pages, panels, task-detail tabs, settings sections, dashboard widgets) loaded into the React app at runtime.

Design constraints from the request:
- **Backend extensions = WASM**, best-in-class tooling.
- **Frontend extensions = Module Federation.**

Hard requirement inherited from the codebase: the workspace is `#[forbid(unsafe_code)]`. That rules out `dlopen`/native `cdylib` plugins and makes **WASM the only sandboxed dynamic-loading option that fits the existing safety posture** — good, it's also the right call independently.

## 2. Best-in-class WASM choice

**Runtime: Wasmtime + the WebAssembly Component Model (WASI Preview 2), interfaces defined in WIT.**

Rationale:
- Wasmtime is the reference Component Model implementation, first-class Rust embedding (`wasmtime` + `wasmtime-wasi`), async support that maps onto our Tokio runtime, fuel/epoch interruption for runaway-guest protection, and per-instance memory limits.
- The **Component Model** (not raw core modules) gives us typed, language-agnostic interfaces via **WIT**. Hosts and guests exchange records/variants/results/strings — no hand-rolled `i32`-pointer ABI. Guests can be authored in Rust (`cargo component` / `wit-bindgen`), but also Go (TinyGo), JS (jco/StarlingMonkey), Python, C — so extension authors aren't forced into Rust.
- WASI P2 gives a **capability-based** sandbox: a component gets *nothing* (no FS, no net, no clock, no env) unless the host explicitly grants it. This is exactly the security model we want for untrusted extensions.

Rejected alternatives: Extism (great DX but core-module/host-function ABI, less expressive than the Component Model and we'd outgrow it); WAMR/wasm3 (embedded-targeted, weaker async + Component Model story); native dynamic libs (violates `forbid(unsafe_code)`, no sandbox).

Pin Wasmtime to a tracked major; the Component Model surface still moves, so we own the WIT and the host bindings and treat the guest ABI as **our** versioned contract.

## 3. Backend architecture

```
                 ┌──────────────────────────────────────────┐
                 │ lazybones-api (Axum)                       │
                 │  /extensions/* CRUD + invoke/proxy routes  │
                 └───────────────┬────────────────────────────┘
                                 │
        ┌────────────────────────┴───────────────────────────┐
        │ lazybones-ext  (new crate)                          │
        │  - Registry (load .wasm from store/blob, validate)  │
        │  - Wasmtime Engine (shared, fuel+epoch+mem limits)  │
        │  - Host impls of imported WIT interfaces (caps)     │
        │  - Extension-point dispatch (find-by-hook, invoke)  │
        └───────┬───────────────────────────────┬─────────────┘
                │ used by                        │ used by
   ┌────────────┴───────────┐        ┌───────────┴────────────┐
   │ lazybones-engine        │        │ lazybones-store         │
   │  scheduler calls ext    │        │  ext records + .wasm    │
   │  hooks at lifecycle pts │        │  blobs (content-addr)   │
   └─────────────────────────┘        └─────────────────────────┘
```

### 3.1 New crate: `lazybones-ext`

Owns the Wasmtime engine, the WIT world, host-side capability implementations, the registry, and the dispatcher. Depends on `lazybones-store` for persistence; `lazybones-engine` and `lazybones-api` depend on it. Keeps WASM concerns out of the scheduler and API crates.

### 3.2 Extension points (the seams that already exist)

Map directly onto the current trait/lifecycle seams found in the codebase:

| Extension point | Current seam | What the WASM guest provides |
|---|---|---|
| **Gate check** | gate script run in worktree before land | given task + worktree summary + diff stat → `pass/fail/skip` + message |
| **Merge strategy** | fast-forward / merge / pr | given branch state → decision/instructions (advisory first, see §6) |
| **Event reaction** | durable `Transition` events + SSE broadcast | subscribe to event kinds → emit follow-up actions (create task, chat, notify). **Cycle-guarded — see §3.4.** |
| **Agent provider** | `hcom` subprocess spawn | declare a provider; host still owns process spawn, guest shapes the invocation/parse |
| **Task mutator / validator** | task create/update | inspect/annotate a task on create; reject with reason |
| **Blob backend** | `Arc<dyn BlobStore>` | *stays native* — perf-critical, not a guest concern (see §6) |
| **Scheduled job** | (none yet) | cron-triggered guest invocation |

Each point is one WIT `interface`. An extension's WIT `world` declares which it `export`s; the registry indexes extensions by exported interface so dispatch is "find all gate-check extensions, invoke in order."

### 3.3 Host capabilities (what guests may `import`)

Default-deny. A guest only gets capabilities its manifest requests *and* an admin grants at install time:

- `log` (always) — structured tracing into the daemon.
- `store-read` / `store-write` — scoped, typed access to tasks/runs/skills (NOT raw SurrealDB; a narrow WIT facade).
- `http-fetch` — outbound HTTP with an allowlist of hosts.
- `secrets-read` — named secrets only, decrypted by host, never the whole vault.
- `kv` — per-extension namespaced key/value scratch space.
- `emit-event` — append an extension-namespaced event.

No raw FS, no raw sockets, no clock-as-entropy. WASI P2 makes these grants explicit and auditable.

**Capability *interactions* matter as much as individual grants.** `secrets-read` + `http-fetch` is a classic exfiltration pair (read a named secret → POST it to an allowlisted host); the host-allowlist is the *only* thing between them, so granting both must raise a louder install-time warning than either alone. The grant UI should flag known-dangerous combinations, not just list caps. Note also that we decrypt secrets into guest linear memory we can't subsequently inspect — acceptable for trusted/signed extensions, a reason to withhold `secrets-read` from untrusted ones entirely.

### 3.4 Lifecycle & resource safety

- One shared `Engine`; instantiate per-invocation (or pooled) `Store` with: **fuel limit** (CPU bound), **epoch interruption** (wall-clock deadline via a background ticker), **memory limiter** (max linear memory), and a host-enforced **call timeout**. A guest that loops/leaks is killed, the hook records a failure, the task is unaffected (fail-open or fail-closed is per-extension-point policy — gate checks fail-closed, event reactions fail-open).
- Guest panics/traps are caught at the host boundary → logged as an extension fault, never propagate into the scheduler tick. This preserves the "every tick rebuilds from store + git + hcom" robustness that already exists.
- **Async scope is deliberately the conservative one.** "All async" here means **Wasmtime async *host functions*** — components run via `call_async` with epoch yielding, and outbound I/O goes through `wasi:http` — so guest I/O doesn't block a Tokio worker. It does **not** mean WIT-level async (`stream`/`future` types, async exports), which is much newer and churnier; P0 must stay on the host-function model and explicitly avoid the WIT async surface, or it will hit Component-Model churn faster than budgeted.
- **Per-extension circuit breaker (not just per-call policy).** Per-call fail-open/closed handles *one* bad invocation but not a guest that is *consistently* wrong without faulting — e.g. a `fail-closed` task-mutator that rejects every task-create bricks task creation for everyone. So each extension carries a breaker: **N consecutive faults (or rejections at a fail-closed point) → auto-disable + surfaced alert.** Policy is stated per extension point, because "fail-open vs fail-closed" alone doesn't save you from a deterministically-wrong guest. Designed in P1, not discovered later.
- **Reentrancy / cycle guard for events.** Guests get both event subscription *and* `emit-event`, which is a feedback loop (A emits → A wakes; or B→A→B). This repo already has a documented `auto_pr` infinite-spawn flaw, so the guard is mandatory from P1: extension-emitted events carry an **origin tag + emission depth**; an event whose causal chain re-enters the same extension beyond a small depth is dropped, and every extension has a **per-window emit rate limit**. Cycles are designed out, not found in production.
- **Instantiation cost is a measured P0 input.** Per-invocation instantiation is fine for rare gate checks but may be hot for event reactions. If we pool, `PoolingAllocationConfig` is the lever; P0 must measure cold instantiation latency, since it decides whether the fail-open event path can afford per-invocation `Store`s or needs a pool.

### 3.5 Storage & distribution

- Extension `.wasm` is a **content-addressed blob** via the existing `BlobStore` (SHA-256 key) — same mechanism as assets.
- A new store model `Extension { id, name, version, wit_world, requested_caps, granted_caps, wasm_sha256, enabled, source }` plus a `frontend` descriptor (see §4).
- Install sources: upload, URL, or a future registry. Manifest (`lazybones.ext.toml`) declares name/version/extension-points/capabilities/frontend-entry and is embedded in the component as a custom section *and* mirrored into the `Extension` store record on install.
- **The manifest has two homes — define the authority.** The embedded custom section is the source of truth for *declared* identity/caps; the store record's `granted_caps` is the source of truth for what an admin *allowed*. On any conflict between the embedded section and the record's `requested_caps`, the **embedded (and, once signing exists, signed) section wins** and a mismatch forces re-review rather than silently trusting the record. `granted_caps ⊆ requested_caps` is enforced at grant time.

### 3.6 API surface (Axum, follows the verb-per-file pattern)

- `POST /extensions` (install: upload/url) · `GET /extensions` · `GET /extensions/:id` · `DELETE /extensions/:id`
- `POST /extensions/:id/enable` · `/disable` · `POST /extensions/:id/grants` (set granted caps)
- `POST /extensions/:id/invoke` (manual/test invoke a named export)
- Frontend asset proxy: `GET /extensions/:id/frontend/*` serves the federated remote's files (see §4.3).

### 3.7 The store facade is a versioned public API (treat it as such)

`store-read` / `store-write` are described in §3.3 as "a narrow WIT facade," but that one line hides **the single most expensive long-term commitment in this design** — more so than Wasmtime churn. Every guest compiled against the facade pins its shape; meanwhile our internal SurrealDB models (`task`, `run`, `skill`) evolve underneath it. The facade is therefore a permanent, independently-versioned public contract, not an implementation detail, and it gets its own section:

- **Shrink the v1 surface: start read-only.** `store-write` is *deferred*. v1 ships `store-read` over a deliberately small projection of `task` and `run` (the fields a gate check / event reaction actually needs), not the whole model. Write-back happens through the typed extension-point return values (e.g. a gate verdict, an emitted event), not arbitrary mutation. This keeps the v1 contract minimal and reversible.
- **Project, don't expose.** The facade returns purpose-built WIT records (`ext-task-view`, `ext-run-view`), explicitly mapped from the internal model — never the SurrealDB row. Internal refactors stay free as long as the projection mapping is updated; the guest contract is untouched.
- **Version the facade independently of the WIT *world*.** The facade interface carries its own semver (`lazybones:store-view@1.x`); a guest declares the range it needs. We can add fields (minor) freely; removing/retyping is a major bump with a deprecation window where both versions are served.
- **Migration story is part of the contract.** When a projection field must change, the old facade version keeps resolving (best-effort default or documented `none`) for one major-version window; the extensions UI flags guests pinned to a deprecated facade so authors can migrate before removal.

## 4. Frontend architecture (Module Federation)

The host app is **React 19 + Vite 6 + Tailwind 4 + Radix**. Use **`@module-federation/vite`** (the actively maintained MF plugin for Vite/Rspack) to make the lazybones UI a **federation host**, and extensions ship as **remotes**.

### 4.1 Model

- **Host** exposes a stable, versioned set of **shared singletons** (`react`, `react-dom`, the query client, the design-system/Radix primitives, an `@lazybones/ext-sdk`) so remotes don't bundle their own React and so styling stays consistent.
- A **remote** is a separately built bundle exposing one or more **mount points**. It's loaded lazily at runtime — no host rebuild to add an extension.
- An **Extension Host SDK** (`@lazybones/ext-sdk`) gives remotes typed access to: the REST client, current route/task context, the event/SSE stream, toast/notify, theme tokens, and **slot registration**.

### 4.2 UI extension slots

Host declares named slots; remotes register components into them:

- `route` — a top-level page + nav entry.
- `task-detail.tab` — a tab on the task detail view.
- `dashboard.widget` — a card on the dashboard.
- `settings.section` — a settings panel.
- `workflow.action` / `task.action` — a button/menu action.

Slot contract is a small typed registry the SDK wraps, so a remote calls `registerSlot('task-detail.tab', {...})` and the host renders it where appropriate. Slots are the frontend mirror of backend extension points.

### 4.3 Loading & serving

- Backend serves each enabled extension's frontend remote (its `remoteEntry.js` + chunks) under `GET /extensions/:id/frontend/*`, with the bundle stored as blobs alongside the `.wasm`.
- On boot, the host fetches `GET /extensions?frontend=1`, then dynamically registers each remote's URL with the MF runtime and imports its mount module. Failures isolate to that remote (error boundary per slot) — a broken extension never white-screens the app.
- Version negotiation: host advertises an SDK semver; remotes declare a compatible range; mismatches are surfaced in the extensions UI and the remote is not mounted.

### 4.4 Tauri note

The desktop wrapper (Tauri 2) feature-detects the bridge already and loads the same web bundle, so federated remotes work unchanged **as long as remotes are served over http(s)** by the daemon (they are, via §4.3) rather than from the Tauri asset protocol. Note for review: confirm Tauri's CSP allows the daemon origin as a script source — **and** that `script-src` permits the MF runtime's dynamic `import()` of remote-entry URLs, which is the same CSP surface and easy to miss.

### 4.5 The frontend plane has *no* sandbox — and that breaks the backend's security model

This is the design's sharpest asymmetry and must be decided before any untrusted-extension promise is made. The backend goes to great lengths — default-deny, WASI P2, explicit grants, capped fuel/memory — to contain a guest. The frontend plane, as described in §4.1–§4.3, does the **opposite**: a federated remote is **arbitrary JavaScript loaded into the host origin**, handed the `@lazybones/ext-sdk` with the live REST client and the user's session. Such a remote can call **any** API the user can, exfiltrate the session token, and rewrite the DOM. For an untrusted third-party extension, **the WASM sandbox is moot if the same extension's UI half is fully-trusted in-origin code.** The whole point of MF's shared singletons (in-origin React/query-client dedup) is precisely what denies us isolation.

There is no free fix. The three options, with trade-offs:

1. **First-party / signed-only frontends** (even if the backend later allows untrusted backends). Simplest; keeps the in-origin MF model and its dedup; the cost is that "untrusted extension" can only ever mean its *backend* half until option 2 lands.
2. **Iframe-sandboxed remotes** (`<iframe sandbox>` + `postMessage` bridge instead of in-origin MF). Much stronger isolation — the remote can't touch the session or DOM directly — but it **fights Module Federation's shared-singleton model**: you lose the React/query-client dedup that §4.1 sells as the whole benefit, and every host capability the remote needs must be marshalled across `postMessage`. This is a different frontend architecture, not a config flag.
3. **Accept full trust** and gate *all* frontend extensions behind the same trust decision as a `secrets-read` backend grant — i.e. frontends are never "untrusted," full stop, and the UI says so.

**Recommendation:** ship option 1 (first-party + signed-only frontends) for v1, design the SDK boundary so an option-2 iframe bridge can be slotted in later for untrusted frontends, and **never advertise untrusted third-party extensions until that bridge exists.** Capability-creep (§7) is about widening backend imports; this is the deeper hole — the frontend has *no* capability model at all today.

## 5. Phasing

1. **P0 — Spike (timeboxed):** stand up `lazybones-ext` with Wasmtime + a single WIT world for **gate-check**, using **async host functions only** (no WIT-level async). Author one Rust guest, run it in a real gate, prove fuel/epoch/memory limits + trap isolation, **and measure cold instantiation latency** to decide per-invocation vs pooled `Store`. No API/UI yet. *Decision gate: is the Component Model DX acceptable for our extension authors?*
2. **P1 — Backend MVP:** registry + store model + blob storage + capability grants (`log`, **read-only `store-read`** over a projected view §3.7, `http-fetch`) + `/extensions` CRUD + dispatch wired into the scheduler for **gate-check** and **event-reaction**. Includes, as core not polish: **per-extension circuit breaker**, **event cycle guard (origin tag + depth + rate limit)**, and fail-open/closed policy stated per point (§3.4). **Signing/verification lands here, not P3, *if* Q1 answers "untrusted from day one"** — the two are coupled.
3. **P2 — Frontend MVP:** make the UI a MF host, ship `@lazybones/ext-sdk`, implement `route` + `task-detail.tab` slots, serve remotes from the daemon, build one end-to-end example extension (WASM gate + a UI tab that shows its results). **First-party/signed-only frontends (§4.5)**; design the SDK boundary so an iframe bridge can be added later. Spike the React/query-client dedup config early — it's the main frontend risk.
4. **P3 — Breadth:** more extension points (merge advisory, scheduled jobs, agent provider), more slots (dashboard widget, settings, actions), `secrets-read` + `kv` capabilities, deferred `store-write`, and signing/verification *if not already pulled into P1 by Q1*.
5. **P4 — Distribution:** an extension registry/marketplace, version channels, an author CLI (`cargo component`-based template + `vite` remote template), and — only if untrusted third-party frontends are ever promised — the **iframe-sandboxed remote bridge (§4.5 option 2)**. Docs.

## 6. Explicit non-goals / boundaries (for v1)

- **Blob backends stay native.** They're on the hot path and an `Arc<dyn BlobStore>` swap is a compile-time concern, not an untrusted-guest concern.
- **Merge strategies start advisory.** A WASM guest *recommends*; the host executes git. We do not hand raw git/worktree mutation to guests in v1.
- **No raw SurrealDB access** from guests — only the narrow typed store facade.
- **No in-process native plugins** ever (`forbid(unsafe_code)`).
- Agent execution stays host-owned (process spawn); guests shape invocation/parsing, they don't spawn processes.

## 7. Risks

- **Frontend trust asymmetry (the #1 risk).** The federated-remote model loads untrusted JS in-origin with full session/API access, defeating the backend sandbox for any extension that has both halves. Mitigation: first-party/signed-only frontends in v1, iframe bridge before any untrusted-frontend promise. See §4.5 — this outranks capability creep below.
- **Store-facade as a permanent public API** — versioned projection that internal models evolve under; the real long-term maintenance cost, bigger than Wasmtime churn. Mitigate: read-only + projected views + independent semver + a migration window (§3.7).
- **Component Model churn** — WIT/Wasmtime APIs still evolve; mitigate by owning the WIT contract, pinning Wasmtime, and staying on async *host functions* (not WIT-level async) (§3.4).
- **MF + Vite + React 19 shared-singleton config** is fiddly; getting `react`/`react-dom`/query-client dedup right is the main frontend risk — spike it early in P2.
- **Capability creep** — every new host import widens the attack surface; keep grants explicit and audited.
- **Author DX** — two toolchains (wasm guest + federated remote) per full extension; the P4 templates/CLI are what make this usable, not optional polish.

---

## Open questions (for when you're back at your PC)

**Answer Q1 first — it gates signing timing (P1 vs P3), the frontend isolation model, and `secrets-read` policy. Everything else can follow.** Where I have a recommendation it's marked → ; the rest are genuinely yours.

1. **Trust model:** first-party-only at launch, or untrusted third-party from day one? → *Recommend first-party + signed-only at launch; design the sandbox for untrusted but don't promise it until §4.5 option 2 exists.* This is the load-bearing decision.
2. **Frontend isolation (coupled to Q1, but a distinct call):** for untrusted frontends, in-origin MF behind signing (option 1), iframe + postMessage bridge (option 2), or full-trust-only (option 3)? See §4.5. → *Option 1 for v1, with the SDK boundary shaped to drop in option 2 later.*
3. **Author language:** Rust-only for v1, or multi-language (Go/JS/Python) at launch? → *Rust-only v1; multi-language is a Component-Model capability, not a v1 obligation — defer the test-matrix cost.*
4. **Coupling:** must a "full" extension bundle both halves, or are backend-only and frontend-only independently valid? → *Both-halves-optional (my assumption); keep it.*
5. **Merge strategies:** advisory-only in v1 (guest recommends, host executes git), or guest-driven merges later? → *Advisory v1.*
6. **Distribution:** in-app upload/URL for v1 with registry/marketplace at P4 — or is the registry a launch requirement?
7. **Capability set:** beyond `log`/`store-read`/`http-fetch`/`secrets-read`/`kv`, any cap you already know you'll need (GitHub via `lazybones-gh`, sending chat to agents)?
8. **MF tooling:** OK to standardize on `@module-federation/vite`? → *Yes; don't migrate the UI to Rspack now — that's a large unrelated move, revisit only if the §7 dedup risk actually bites in P2.*
