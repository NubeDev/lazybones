# Green-build gate is daemon-global; make it per-workflow

**Labels:** enhancement, engine, api
**Severity:** high — one daemon can only serve a single repo stack

## Summary

The green-build gate is a **daemon-global** setting (`EngineConfig.gate`, env
`LAZYBONES_GATE`), defaulting to:

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

A workflow's `workspace` carries `repo`, `base_branch`, `branch_prefix`,
`worktree_mode`, `tool`, `model`, `effort` — but **no gate**. So every workflow on a
given daemon is gated by the same (Rust) commands. Point a workflow at a
Node/Python/Go repo and every task fails the gate, blocks, and dead-ends the run.

## Observed

`web-demo` workflow targeting a Node repo:

- Agents completed their work and transitioned `running → gating`.
- Gate ran `cargo test --workspace` in the Node worktree → failed:
  `error: could not find Cargo.toml in /…/.lazy/wt/backend or any parent directory`.
- Both `backend` and `frontend` → `blocked`.
- `test-be` (deps `backend`) and `test-fe` (deps `frontend`) stranded `pending`
  forever, since deps can never reach `done`.
- Workflow state: `needs-attention`, 0/4 done.

The gate behaved *correctly* (it refused to land an unverifiable build) — the bug is
that there's no way to give a workflow the right gate for its stack.

## Where

- `crates/lazybones-engine/src/config.rs` — `gate: Vec<String>` on `EngineConfig` (global).
- `crates/lazybones-engine/src/scheduler/gate.rs` — `gate::run` uses that global list.
- `crates/lazybones-engine/src/scheduler/effective.rs` — existing task→run→global
  resolution for git/agent settings (the pattern to mirror).
- `crates/lazybones-api/src/dto.rs` — `CreateWorkflowBody.workspace` has no `gate`.
- `crates/lazybones-store/src/run/model.rs` — `Run`/`Workspace` model to extend.

## Proposed fix

1. Add optional `gate: Option<Vec<String>>` to the workflow `workspace` (DTO + `Run`
   model + store row + migration).
2. Resolve the **effective gate** as `task → workflow → global default`, mirroring
   `EffectiveGit`. Have `gate::run` consume the resolved list.
3. Semantics: `gate: []` (explicit empty) = **no gate** (auto-pass after agent DONE);
   `gate: null`/absent = inherit global default.

## Acceptance criteria

- [ ] A workflow can specify its own gate commands (e.g. `["npm test --prefix backend"]`).
- [ ] One running daemon can serve two workflows with different gates simultaneously.
- [ ] Empty gate list lands the branch without running any gate command.
- [ ] Absent gate falls back to the global `EngineConfig.gate`.

## Related

See `docs/issues/01-headless-claude-trust-permission-gates.md` — surfaced in the same
run; both block using lazybones on non-Rust repos with headless agents.
