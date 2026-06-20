# Headless Claude Code agents stall on trust/permission gates

**Labels:** bug, engine, hcom
**Severity:** high — blocks every fully-headless agent run

## Summary

When the scheduler spawns a headless Claude Code agent into a fresh git worktree,
the agent stalls on Claude Code's interactive gates and does no work. The lazybones
task sits `running` until the 3600s await timeout, then blocks — producing nothing,
no commit, no error in lazybones itself.

## Symptoms

`hcom list --json` shows the agent blocked, in two stages:

```json
{ "name": "backend-beni", "status": "blocked", "status_context": "launch_blocked", "tag": "backend" }
// ...later, after trust is granted:
{ "name": "backend-momo", "status": "blocked", "status_context": "approval", "tag": "backend" }
```

The agent's background log shows it waiting on a TUI prompt:

```
Accessing workspace: /home/user/code/rust/testing/.lazy/wt/backend
Quick safety check: Is this a project you trust? ...
  1. Yes, I trust this folder
  2. No, exit
Enter to confirm · Esc to cancel
```

## Root cause

Two distinct Claude Code gates are not bypassed by the headless launch:

1. **Folder-trust dialog** — fresh worktree paths (`.lazy/wt/<task>`) are not in
   `~/.claude.json`'s trusted `projects`, so Claude prompts on first access and hangs.
2. **Per-tool approval** — once trusted, every `Bash`/`Write`/`Edit` triggers an
   "allow?" prompt; with no terminal attached the agent freezes (`approval`).

`hcom 1 claude --go --headless` skips **hcom's** confirmation but not **Claude
Code's** own gates.

## Where

`crates/lazybones-engine/src/hcom/spawn.rs` — `Hcom::spawn()` builds the launch
command and only forwards `--model` / `--effort` to the tool CLI:

```rust
cmd.arg("1").arg(tool)
   .arg("--tag").arg(tag)
   .arg("--dir").arg(dir)
   .arg("--go").arg("--headless")
   .arg("--hcom-prompt").arg(prompt);
if let Some(model)  = model  { cmd.arg("--model").arg(model); }
if let Some(effort) = effort { cmd.arg("--effort").arg(effort); }
```

## Proposed fix

When `tool == "claude"`, forward a permission-bypass flag to the CLI:

- `--dangerously-skip-permissions` (covers both trust and tool-approval), or
- `--permission-mode <mode>` if a softer posture is wanted.

Make the posture **configurable per tool** (each agent CLI has a different flag), e.g.
a `permission_flags: Map<tool, Vec<String>>` on `EngineConfig` resolved in `spawn()`.
Do not hard-code a single CLI's flag.

## Repro

1. Create a workflow on a repo whose worktrees aren't pre-trusted.
2. `POST /workflows/:id/start`.
3. `hcom list --json` → agents `blocked: launch_blocked`, then `approval`.
4. Tasks never progress; no commits.

## Current workaround (not a fix)

- Pre-trust worktree paths in `~/.claude.json` (`projects[path].hasTrustDialogAccepted = true`).
- Commit a `.claude/settings.json` allow-list in the target repo (inherited by
  worktrees branched from base):
  ```json
  { "permissions": { "allow": ["Bash","Edit","Write","Read","Glob","Grep","WebFetch"] } }
  ```

Both are fragile and per-repo; the fix belongs in the spawn path.

## Acceptance criteria

- [ ] A headless agent spawned into a brand-new, never-trusted worktree runs to
      completion with no manual `~/.claude.json` or per-repo `.claude/settings.json`.
- [ ] Permission posture is configurable, not hard-coded to one CLI's flag.
- [ ] Other tools (codex, gemini, opencode) unaffected / get their own mapping.
