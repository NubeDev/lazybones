//! Orchestration tools — tasks, skills, templates, workflows (design §6.1).
//!
//! Authoring verbs (`workflow.create`/`add_task`, `task.create`/`update`,
//! `template.*`, `skill.*`) check `Capability::Author`; reads need none. Lifecycle
//! verbs (`workflow.start` → `Claim`; `workflow.stop`/`resume`/`restart` and
//! `task.retry`/`auto_retry`/`cancel` → `Block`) are present but gated, so the
//! default management (`Author`) token authors then hands back — it cannot start a
//! run. `follow_up.file` is the agent's "needs a human" escape hatch.
//!
//! Scaffold: no tools yet (task `mcp-crate`); the §6.1 set lands in `mcp-spike` /
//! `mcp-orchestrate`.
