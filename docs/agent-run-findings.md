# Findings from a live agent run (web-demo)

> Captured 2026-06-20 while exercising a 4-task workflow (backend, frontend,
> test-be, test-fe) on a Node target repo with Claude Code (sonnet).
> Two product gaps surfaced; both are blockers for using lazybones on
> non-Rust repos or with fully headless agents.

## Finding 1 — Headless Claude Code agents stall on trust/permission gates

**Problem.** When the scheduler spawns a headless Claude Code agent into a fresh
git worktree, the agent stalls on Claude Code's interactive gates and does no work.
`hcom list --json` shows the agent `blocked` with `status_context: launch_blocked`
(folder-trust) and later `approval` (per-tool permission).

Two distinct gates block headless runs:
1. **Folder-trust dialog** — *"Is this a project you trust?"*. Fresh worktree paths
   (`.lazy/wt/<task>`) aren't in `~/.claude.json`'s trusted projects, so Claude
   prompts and hangs.
2. **Tool-approval prompts** — every `Bash`/`Write`/`Edit` asks *"allow?"*; with no
   terminal the agent freezes (`blocked: approval`).

`hcom 1 claude --go --headless` skips hcom's own confirmation but does **not** bypass
Claude Code's gates. The task sits `running` in lazybones until the 3600s await
timeout, producing nothing.

**Where.** `crates/lazybones-engine/src/hcom/spawn.rs` — `Hcom::spawn()` builds the
launch command but only forwards `--model` / `--effort`.

**Proposed fix.** When `tool == "claude"`, pass `--dangerously-skip-permissions`
(and/or `--permission-mode`) through to the CLI so headless agents run autonomously.
Better: make the permission posture configurable per tool in `EngineConfig`, since
each agent CLI has a different flag.

**Current workaround (used in this run).** Pre-trust worktree paths in
`~/.claude.json` and commit an allow-list `.claude/settings.json` in the target repo
(inherited by worktrees branched from the base). Fragile and per-repo; belongs in the
spawn path.

## Finding 2 — Green-build gate is daemon-global; should be per-workflow

**Problem.** The gate is a daemon-global setting (`EngineConfig.gate`, env
`LAZYBONES_GATE`), defaulting to `cargo test --workspace` +
`cargo clippy --workspace --all-targets -- -D warnings`. A workflow's `workspace`
carries `repo`, `base_branch`, `tool`, `model`, `effort` — but **not** a gate.

So one daemon can only serve repos of a single stack. Pointing a workflow at a
Node/Python/Go repo makes every task fail the gate (`could not find Cargo.toml`),
which blocks the task; dependent tasks then never promote, dead-ending the workflow.

**Observed.** `web-demo` on a Node repo: agents completed their work, transitioned
`gating`, then `cargo test --workspace` failed → `blocked`. `test-be`/`test-fe`
(deps on the blocked tasks) stranded `pending` forever. Workflow state
`needs-attention`, 0/4 done.

**Where.**
- `crates/lazybones-engine/src/config.rs` — `gate` lives on `EngineConfig` (global).
- `crates/lazybones-engine/src/scheduler/gate.rs` — `gate::run` uses that global list.
- `crates/lazybones-api/src/dto.rs` — `CreateWorkflowBody.workspace` has no `gate`.

**Proposed fix.** Add an optional `gate: Vec<String>` to the workflow `workspace`
(and the `Run` model); have `gate::run` resolve effective gate as
**task → workflow → global default** (mirroring the existing `EffectiveGit`/agent
resolution). `gate: []` means "no gate". Lets one daemon serve mixed-stack repos.
