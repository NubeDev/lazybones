---
name: retry-resume-design
description: How task retry/resume/auto-retry work, the revive-vs-reset distinction, and the stop/resume run lifecycle
metadata:
  type: project
---

Workflow retry/resume capability (added 2026-06-21, branch feat/engine-scheduler).

## Run lifecycle: stop / stop-reset / resume (added 2026-06-21)

`Lifecycle` (stored, run/model.rs) is now `active | stopped` — the terminal
`cancelled` tombstone was **dropped**. Only `done` (derived) and a hard `delete`
are terminal; `stopped` is a reversible pause. `Lifecycle::parse` maps legacy
`"cancelled"` → `Stopped`. `derived_state` (run/derived.rs) precedence is now
`done → stopped → needs-attention → running → ready → draft` (done wins over
stopped: an all-done run has nothing to resume). Store verbs: `stop_run`/
`resume_run` (run/stop.rs share `set_lifecycle`, run/resume.rs); old `cancel_run`
removed.

**Scheduler guard** (the core bug fix — a "cancelled" run used to keep running):
a stopped run promotes/claims NOTHING. `newly_ready(db, stopped_runs: &[String])`
(task/depend.rs) excludes pending tasks whose `run_id` is in the stopped set
(`run_id = NONE OR run_id NOT IN $stopped`); `handle.stopped_run_ids()` supplies
it. `scheduler::tick::promote` passes it; `claim_and_spawn` also `continue`s when
the cached parent run's lifecycle != Active. Tested in tick_walk_test.rs
(`stopped_run_claims_nothing`).

**409 guard** closing the bug at the API: the task-level revive verbs refuse when
the parent run isn't `active` — `routes/guard.rs::ensure_run_revivable` is called
from `tasks_retry`, `tasks_retry_policy` (auto-retry), and `chat` (post_chat).
`workflows/:id/resume` is the escape hatch (it flips lifecycle → active first), so
it's exempt. Tested in workflows_test.rs (`stop_pauses_and_blocks_revive_until_resume`).

**Routes** (routes/mod.rs, one-file-per-route, `Block` cap): `POST /workflows/:id/
stop` (workflows_stop.rs — pause + reclaim running→ready, keep work), `POST
.../stop-reset` (workflows_stop_reset.rs, renamed from the old `cancel` — pause +
reset unfinished→pending), `POST .../resume` (workflows_resume.rs — now flips
lifecycle→active AND resets blocked→pending). `restart` unchanged (resets without
pausing).

**UI**: Cancel → a **Stop** button opening a dialog with "Stop (keep work)" /
"Stop & reset" (workflow-controls.tsx); `terminal = state === "done"` only (stopped
is NOT terminal); Resume shown when `stopped || needs-attention`; a prominent
stopped banner in workflow-detail.tsx. Types: `Lifecycle = active|stopped`,
`WorkflowState` drops cancelled → adds stopped (types/workflow.ts +
workflow-state-meta.ts). API/hooks: `stopWorkflow`/`stopResetWorkflow` +
`useStopWorkflow`/`useStopResetWorkflow` (replaced cancel). Docs updated:
workflows-scope.md, managing-with-ai.md.

Two distinct revive mechanisms exist for a `blocked` task — do not conflate them:
- **Clean reset** (`store.reset`): `blocked → pending`, CLEARS worktree/claim/
  reason and zeroes `retry_count`. For transient/flaky failures. Used by
  `POST /tasks/:id/retry` (no `strategy`), `POST /workflows/:id/resume` (all
  blocked tasks), and `POST /workflows/:id/restart`.
- **Guided revive** (`store.revive_with_guidance` → `Transition::Revive`):
  `blocked → ready`, KEEPS the worktree, appends a `RetryStrategy` guidance blurb
  as a `role=user` chat message so `scheduler::prompt::compose` folds it into the
  re-spawn prompt. For "it failed for a reason, here's how to fix it."

`RetryStrategy` (`long_term` | `quick`, in `lazybones-store` task/model.rs) is the
unifying primitive — same guidance drives manual retry AND hands-off auto-retry.

**Auto-retry**: per-task policy (`Task.auto_retry`/`max_retries`/`retry_count`,
default cap `DEFAULT_MAX_RETRIES = 2`). The hook is in `scheduler::finish::block`
(the single choke point for ALL end-of-run failures): block first, then if
`auto_retry` set and `retry_count < max_retries`, revive-with-guidance and bump
the counter (`bump_count=true`). Manual retry passes `bump_count=false` (uncapped
— a human is in the loop). Set the policy via `PUT /tasks/:id/auto-retry`.

This complements the chat revive (`POST /tasks/:id/chat` on a blocked task), which
uses `Transition::Revive` directly with free-text guidance. The chat UI now exists:
`ui/src/features/tasks/detail/task-chat.tsx` (bubbles + composer, Enter to send,
surfaces the `delivery` result incl. "revived"), `lib/api/chat.ts` + `lib/hooks/
use-chat.ts` (query key `["chat", id]`), wired into the task inspector as a "Chat"
section. Live updates via a `chat` SSE listener in `use-live-stream.ts` that
invalidates `["chat"]`. A done task disables the composer.

UI: the retry-on-fail controls live in ONE shared component
`ui/src/features/tasks/detail/task-retry-controls.tsx` — Fix long-term / Quick fix
/ Re-run clean buttons (when blocked) + an auto-retry off/long-term/quick toggle
with an inline max-cap stepper (1–10, re-sends the active strategy with the new
`max_retries`). It is rendered in BOTH the task inspector (`task-detail.tsx`, a
"Retry on fail" section) and the workflow Tasks tab (`workflow-tasks.tsx`). Resume
button in `workflow-controls.tsx` (header) AND a prominent needs-attention banner
in `workflow-detail.tsx` (shown when blockedCount > 0). Blocked Board cards
(`task-card.tsx`) host compact Fix/Quick quick-retry buttons (stopPropagation so
they don't select/drag; blocked cards are non-draggable anyway). So retry-on-fail
is reachable from board cards, the inspector, the Tasks tab, and the workflow
banner.
