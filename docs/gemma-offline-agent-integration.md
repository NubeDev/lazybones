# Offline Gemma agent in lazybones — integration plan

**Goal:** drive lazybones tasks with a fully offline, local Gemma model instead of (or alongside) cloud Claude.

**Verdict:** feasible *without writing a new agent runtime, and without changing the harness*. The cleanest
path keeps the **unchanged `claude` harness** and only swaps the model behind it, by pointing Claude Code at
a local [mistral.rs](https://github.com/ericlbuehler/mistral.rs) server that exposes an **Anthropic-compatible
`/v1/messages` endpoint** serving Gemma 4. Lazybones change is essentially **one env-var injection**.

---

## 1. Why this works (architecture facts)

Lazybones never invokes `claude` directly. The scheduler issues:

```
hcom 1 <tool> --tag <task-id> --dir <wt> --go --headless --hcom-prompt <prompt> [--model …] [--effort …]
```

- `<tool>` resolves **task → workflow → global** (default `"claude"`), see
  [effective.rs:106-172](../crates/lazybones-engine/src/scheduler/effective.rs#L106-L172).
- Spawn command + **env injection** built in [spawn.rs:40-114](../crates/lazybones-engine/src/hcom/spawn.rs#L40-L114).
  Today it *strips* `CLAUDE_CODE_*` vars; we add an offline-mode injection here (see step C).
- Completion is **hcom-level**, not Claude-specific: explicit `DONE`/`BLOCKED` message, or an
  idle/exit fallback (`IDLE_DONE_SECS`), see [finish.rs:122-250](../crates/lazybones-engine/src/scheduler/finish.rs#L122-L250).
- After the signal, the daemon **auto-commits** the worktree and runs the gate regardless of model
  ([merge.rs](../crates/lazybones-engine/src/scheduler/merge.rs)).

Because the harness stays `claude`, **DONE detection, gates, worktrees, folder-trust, and `--permission-mode auto`
all keep working as-is**. We only redirect where Claude Code sends its inference requests.

---

## 2. Path A — drop-in brain swap via mistral.rs (RECOMMENDED)

### Why mistral.rs
- **Rust** inference engine ([github.com/ericlbuehler/mistral.rs](https://github.com/ericlbuehler/mistral.rs)) —
  same ecosystem as lazybones; embeddable later (see Path C).
- `mistralrs serve` exposes **OpenAI-compatible `/v1` AND Anthropic-compatible Messages endpoints** — the
  Anthropic one is what lets the existing `claude` harness talk to it.
- **Gemma 4 is first-class** (E4B, 26B-A4B benchmarked), multimodal.
- Built-in **tool calling with grammar enforcement + strict schema mode** — critical, since the agent loop
  lives or dies on reliable tool-call formatting.
- Backends: GGUF (2–8 bit), GPTQ, AWQ, FP8, ISQ; CUDA / Metal / CPU; multi-GPU.

### The mechanism
Claude Code honors `ANTHROPIC_BASE_URL`. Point it at the local mistral.rs Anthropic endpoint and the same
harness runs against Gemma — fully offline. Lazybones still spawns `tool=claude`; nothing else in the
scheduler/gate/worktree machinery changes.

```
ANTHROPIC_BASE_URL=http://localhost:<port>
ANTHROPIC_API_KEY=local-dummy          # harness requires the var to be set; value unused locally
tool=claude                            # unchanged
```

### Steps

**A. Local model + server**
1. Install mistral.rs (`cargo install mistralrs-server` or build from source for your backend).
2. Run: `mistralrs serve` with Gemma 4 (e.g. the 26B variant, ISQ/GGUF quant to fit VRAM), Anthropic
   endpoint enabled. Confirm it answers a manual `curl` to the Anthropic `/v1/messages` route **offline**.

**B. Verify Claude Code against it (do this BEFORE touching lazybones)**
3. In a scratch dir, run Claude Code headless with `ANTHROPIC_BASE_URL` pointed at the server and a dummy
   key. Give it a trivial edit task. **This is the make-or-break test** — "Anthropic-compatible" rarely
   means *Claude Code–compatible* on the first try (system-prompt injection, tool-call block formatting,
   streaming, and `cache_control` fields are common mismatch points). Only proceed if this works.

**C. Lazybones env injection (the only code change)**
4. In [spawn.rs:72-102](../crates/lazybones-engine/src/hcom/spawn.rs#L72-L102), when an **offline mode** is
   active for the `claude` tool, inject `ANTHROPIC_BASE_URL` (+ dummy `ANTHROPIC_API_KEY`) into the agent
   subprocess env. Gate it behind a config flag so cloud Claude stays the default — e.g.:
   - config key `agent_base_url` / env `LAZYBONES_AGENT_BASE_URL` in
     [config.rs:59-75](../crates/lazybones-engine/src/config.rs#L59-L75), forwarded per-tool.
   - Keep the existing `CLAUDE_CODE_*` stripping; just *add* the base-url var.
5. Credentials: with a local server there's no real Anthropic key. Ensure the dummy key satisfies the
   harness and that `store.secret_env()` doesn't overwrite/clear `ANTHROPIC_BASE_URL`
   ([spawn.rs:72-102](../crates/lazybones-engine/src/hcom/spawn.rs#L72-L102)).

**D. Select offline mode**
6. Global: set `LAZYBONES_AGENT_BASE_URL=http://localhost:<port>` (tool stays `claude`).
   Or per-task — extend `CreateTaskBody` ([dto.rs:251-307](../crates/lazybones-api/src/dto.rs#L251-L307))
   with an optional base-url override so you can A/B cloud-Claude vs local-Gemma per task.

**E. Tune for a slower model**
7. Local Gemma generates slower than cloud Claude. Review the await/idle timeouts in
   [finish.rs:122-250](../crates/lazybones-engine/src/scheduler/finish.rs#L122-L250) — especially
   `IDLE_DONE_SECS` and `AWAIT_SECS` — so the daemon doesn't infer DONE or time out mid-generation.

### Files to touch (Path A)

| File | Change |
|------|--------|
| (external) mistral.rs server | run Gemma 4, Anthropic endpoint, offline |
| [config.rs:59-75](../crates/lazybones-engine/src/config.rs#L59-L75) | add `agent_base_url` / `LAZYBONES_AGENT_BASE_URL` |
| [spawn.rs:72-102](../crates/lazybones-engine/src/hcom/spawn.rs#L72-L102) | inject `ANTHROPIC_BASE_URL` + dummy key for offline `claude` |
| [finish.rs:122-250](../crates/lazybones-engine/src/scheduler/finish.rs#L122-L250) | raise idle/await timeouts for slow local gen |
| [dto.rs:251-307](../crates/lazybones-api/src/dto.rs#L251-L307) | *(optional)* per-task base-url override |

**Net lazybones code change: one env injection + a config flag.** Everything else is unchanged.

---

## 3. Test plan (do not skip the trivial-first step)

1. **Server smoke (offline):** disconnect network. `curl` the mistral.rs Anthropic `/v1/messages` endpoint;
   Gemma 4 responds with a valid tool-call when prompted to use a tool.
2. **Harness smoke (offline):** Claude Code headless with `ANTHROPIC_BASE_URL` set edits a file and exits
   cleanly in a scratch dir. **Gate everything on this passing.**
3. **One trivial lazybones task:** offline mode on, `tool=claude`, task = "add a doc comment to fn X".
   Watch one full cycle: spawn → edit → DONE → green gate → auto-commit → land.
4. **Tune:** adjust `IDLE_DONE_SECS`/`AWAIT_SECS` until the trivial task lands 5/5.
5. **Real task:** a small bugfix touching 1–2 files with a test. Measure gate pass rate.
6. **A/B:** same task cloud-Claude vs local-Gemma to quantify the capability gap.

---

## 4. Honest expectations / risks

- **The compat-layer risk is the #1 unknown.** If Claude Code doesn't run cleanly against mistral.rs's
  Anthropic endpoint (step B), Path A is dead and you fall back to Path B. Test that *first*, before any
  lazybones change.
- **Capability gap is the real limiter, not plumbing.** Even Gemma 4 26B is far weaker than Claude at
  sustained multi-step tool-use. Small bounded tasks work; expect frequent failure on multi-file features,
  red-gate recovery, and long edit/test loops. mistral.rs makes the wiring excellent — it doesn't make
  Gemma as smart as Claude.
- **Existing headless fragilities amplify.** Park hangs, DONE reconcile lag, idle-inference edge cases
  (see project memory) all get worse with a slower, less reliable model.
- **Hardware:** Gemma 4 26B at usable speed wants a strong GPU (24GB+ VRAM) or aggressive quant; slow gen
  stresses every scheduler timeout.

**Recommended posture:** keep cloud Claude as default; enable offline Gemma via a config flag (or per-task
base-url) for offline/low-stakes work. A global offline-only switch is achievable but will visibly reduce
throughput and reliability.

---

## 5. Path B — fallback: local harness + Ollama (only if Path A's compat test fails)

If Claude Code won't drive mistral.rs's Anthropic endpoint, swap the **harness** instead of the brain. This
is more work and uses a weaker harness, but avoids the Claude-Code-compat dependency.

- **Harness: opencode** — already in lazybones' catalog ([catalog.rs:26-87](../crates/lazybones-api/src/engine/catalog.rs#L26-L87)),
  best Ollama/OpenAI-compatible support, MCP + LSP, actively maintained (Aider stalled since Aug 2025).
- **Model: Gemma 4** via Ollama (`ollama pull`), served at `localhost:11434`. (Plain Gemma 3 is too weak at
  tool-calling; `orieg/gemma3-tools:27b` is a fallback fine-tune — avoid the 4b.)
- mistral.rs's **OpenAI-compatible `/v1`** endpoint also works here as a higher-quality alternative to
  Ollama for the same opencode harness.
- Lazybones changes: confirm `opencode` registered in **hcom's** tools map; allow a **no-credential** local
  tool in [spawn.rs:72-102](../crates/lazybones-engine/src/hcom/spawn.rs#L72-L102); add a `permission_flags`
  entry for `opencode` in [config.rs:70-75](../crates/lazybones-engine/src/config.rs#L70-L75); confirm
  `--model`/`--effort` forwarding; set `LAZYBONES_AGENT_TOOL=opencode`.
- Completion: opencode won't post `DONE` to hcom — you lean on the idle/exit fallback
  ([finish.rs:140-150](../crates/lazybones-engine/src/scheduler/finish.rs#L140-L150)), the most fragile path.

---

## 6. Path C — endgame: embed mistral.rs in-process

`cargo add mistralrs` and run inference *inside the lazybones daemon* — no external server, no Ollama.
"Lazybones owns its own offline brain." Real engineering, and you'd still need a harness driving it (either
Claude Code via an in-process Anthropic shim, or mistral.rs's own server-side agentic loop, which is weaker
than Claude Code's). Pursue only after Path A is proven valuable.
