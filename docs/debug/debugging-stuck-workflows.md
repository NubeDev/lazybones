# Debugging a stuck workflow (hcom + the scheduler)

> Audience: an operator (or the management agent) watching a workflow that looks
> wedged. Read [scheduler.md](../scheduler.md) for the tick/state-machine model
> first. This doc is the **triage runbook**: how to tell a genuinely-stuck task
> from one that only *looks* stuck, and the handful of walls that actually stall
> agents.

## TL;DR — the one rule

**"`done_count: 0` and nothing moving" is not evidence of a stall.** A workflow in
`shared` mode runs its tasks **sequentially** on one branch: only the
dependency-zero head runs first, and every other task sits `pending` behind it.
A 10-minute implementation task is normal. Before escalating, prove the head task
is *parked* (not *working*) using the triage flow below.

Most "it's stuck again" reports are one of three non-bugs:

1. The head task is **actively working** (`hcom list` shows `active: …`, not
   `listening`) — just slow. Wait.
2. **DONE→reconcile lag** — the task signalled DONE and pushed, but the record
   sits `running` / `commit=null` for ~1–2 min before the daemon reconciles. A
   manual retry here just churns already-committed work. See
   [§ The reconcile-lag trap](#the-reconcile-lag-trap).
3. A **false time signal** — `started_at` is a *start* time, not elapsed-stuck
   time; and a date-rollover artifact has made a ~1-min lag look like ~24h.
   Always check elapsed against `date -u`.

A task is *genuinely* stuck only when it is `running` / `commit=null` for **several
minutes**, the agent is **parked** (`listening`, not `active`), **and** the next
task never auto-promotes.

## Triage flow (run these in order)

Set the daemon base URL once (default dev port shown; yours may differ — check the
daemon banner or `/tmp/lazybones-dev.log`):

```sh
B=http://127.0.0.1:46787
```

### 1. Is the daemon even up?

```sh
curl -sf $B/health        # {"status":"ok"}
```

No response → the daemon is down; the loop isn't ticking. Restart it
(`target/debug/lazybonesd serve`) and re-check.

### 2. What does the workflow think its state is?

```sh
curl -s $B/workflows | python3 -m json.tool
```

Look at the workflow: `state`, `task_count`, `done_count`, `started_at`,
`failed_at`. `state: running` with `done_count: 0` is expected while the head task
runs — not a stall by itself.

### 3. Which task is the chain actually waiting on?

```sh
curl -s $B/tasks | python3 -c "
import sys,json
RUN='doc-writer'      # <- your workflow id
for t in json.load(sys.stdin):
    if t.get('run_id')==RUN:
        print(f\"{t['id']:<22} {t['status']:<8} commit={str(t['commit'])[:8]:<8}\"
              f\" session={t['session']} retry={t['retry_count']}/{t['max_retries']}\"
              f\" auto_retry={t['auto_retry']}\")
        print(f\"   deps={t['deps']} started={t['started_at']} reason={t['reason']}\")
"
date -u                 # <- compare elapsed against THIS, not started_at alone
```

The one `running` task with `commit=null` is the head. Everything `pending` behind
it is just waiting — that's correct, not stuck.

> **`auto_retry: null` is a risk flag.** Auto-retry is what self-heals a task that
> parks after DONE (it re-runs on a fresh session onto the *same* pushed commit,
> zero work lost). A head task with `auto_retry: null` has **no self-heal** — if it
> parks, a human must nudge it. Worth setting a retry policy at authoring time.

### 4. Is the agent *working* or *parked*? (the decisive check)

This is the single most informative command. `hcom` is the source of truth for
"is the agent alive and what is it doing":

```sh
hcom list                       # one line per live agent
hcom list -v                    # + session_id, directory, headless log path
```

Read the head task's line (named `<task-id>-<session>`, e.g. `dw-store-kobe`):

| What you see | Meaning | Action |
| --- | --- | --- |
| `▶ … Ns ago: active: Write` / `Edit` / `Bash` | **Working.** Tool calls are firing. | Wait. Not stuck. |
| `◉ … now: listening` | Idle — *finished or parked*. Cross-check below. | See step 5. |
| (task's agent **absent** from `hcom list`) | The agent died/was reaped. | Reconcile will reclaim it; check the daemon log for `launch_blocked` / `spawn failed`. |

`hcom list -v <task>-<sess>` also gives **Uptime / State Age** — the agent's real
wall-clock age. Use it instead of `started_at` to judge "how long really?".

### 5. If `listening` — finished or parked?

A `listening` agent has either **finished** (signalled DONE, work pushed) or
**parked** on an unanswerable prompt. Distinguish them:

```sh
# Did it actually produce work? Look in the worktree.
cd <repo>/.lazy/wt/<workflow-id>            # shared mode: one wt per workflow
git log --oneline -5                        # a fresh task(...) / "… completed of N" commit?
git status --short                          # uncommitted work still sitting there?

# What is the agent's last screen? (parked prompts show here)
tail -40 ~/.hcom/.tmp/logs/background_*_*.log   # path from `hcom list -v`
```

- **Fresh commit present, record still `running`** → DONE→reconcile lag. **Wait**
  (see next section). Do not retry.
- **Uncommitted work + a Write/approval prompt on screen** → a parked agent. Match
  it against [§ The walls](#the-walls-why-an-agent-actually-parks).
- **No work at all, clean tree** → a genuine no-op or an early spawn-time park.

## The reconcile-lag trap

When a task agent signals DONE and pushes, the REST record can sit at
`status=running`, `commit=null`, `finished_at=null` for **~1–2 minutes** before the
daemon records the done-transition and promotes the next task. **This is normal.**

Two endpoints lie here — do not read a stall into them:

- `GET /runs/:id` (transitions) returns `[]` **even for fully done+committed
  tasks** in this build. Empty transitions carry *no* information. The only
  authoritative tells are the task record's `status` / `commit` / `finished_at`.
- `started_at` is a *start* timestamp. "running since 01:35Z" is not "stuck for
  hours" — confirm against `date -u` and `hcom list -v` Uptime.

**Self-heal:** the first DONE sometimes doesn't reconcile (the headless agent parks
at `listening: uncommitted text` instead of exiting). If the task has an
`auto_retry` policy, the daemon re-runs it on a fresh session and it reconciles
cleanly onto the **already-pushed commit** — zero work lost. So holding off a manual
retry during the lag window is correct. **Proof a task reconciled:** its successor
can't go `ready` unless it did — watch for the next task to auto-promote.

> **Fixed (base_commit baseline):** there *was* a bug where this self-heal stranded
> the work instead. A task records a `base_commit` baseline at claim — "HEAD before
> this task ran" — and the gate flags `blocked: "task produced no commit of its own
> (empty task)"` when HEAD never moves past it. The baseline used to be **re-stamped
> to current HEAD on every (re)claim**, so a task whose first attempt committed green
> work but died in the reconcile lag had its retry reuse a tree whose HEAD *was* that
> commit → the retry adopted the task's own work as its baseline → "HEAD didn't
> advance" → blocked as empty, with the finished commit sitting right there on the
> branch. The fix ([scheduler/tick.rs](../../crates/lazybones-engine/src/scheduler/tick.rs)
> `resolve_base_commit`, [scheduler/worktree.rs](../../crates/lazybones-engine/src/scheduler/worktree.rs)
> `Provisioned.reused`): the baseline is now captured **once, before the first
> attempt**, and **preserved across reclaims onto a reused tree** — so a retry never
> flags already-committed work empty. If you see a historical `empty task` block on a
> task whose worktree *does* hold a `task(...) … completed of N` commit, that is this
> bug; on the fixed daemon, clean-retry it (`remove_worktrees:true` re-cuts from base
> and re-stamps the correct baseline) or just confirm the successor promoted.

Only escalate (human nudge / retry) when `running`/`commit=null` persists **several
minutes** AND the next task never promotes AND `hcom` shows the agent `listening`
(parked), not `active`.

## The walls: why an agent actually parks

When an agent *is* genuinely parked, it's almost always one of these. Each is a
distinct wall with a distinct fix.

| # | Symptom / tell | Cause | Fix |
| --- | --- | --- | --- |
| 1 | Hangs/loops **at spawn**; daemon log `launch_blocked: screen settled` / `spawn failed (exit status: 2)` | Claude Code **bypass-permissions consent** screen (one-time, per host) | Operator, once: run `claude` interactively → "Yes, I accept", or set `bypassPermissionsModeAccepted: true` in `~/.claude.json`. No env var skips it. |
| 2 | Hangs at spawn with a **"Do you trust this folder?"** prompt | Folder-trust gate on the worktree | Auto-handled by `auto_trust_agent_folder` (default on, seeds `hasTrustDialogAccepted`). If off, enable it. |
| 3 | Parks **mid-run**, `commit=null`, screen shows a Write/create-file approval for a `.claude/` or `memory-note` path | Claude Code **auto-memory** trying to write into protected `.claude/` — no `permissions.allow` rule suppresses it | Daemon spawns agents with `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` ([hcom/spawn.rs](../../crates/lazybones-engine/src/hcom/spawn.rs)). Update + restart the daemon. On an old daemon: **deny** the prompt (3/No), never "allow all". |
| 7 | `launch_failed` exit code 1 across **every** repo; headless log shows `error: option '--permission-mode <mode>' argument 'auto' is invalid. Allowed choices are acceptEdits, bypassPermissions, default, delegate, dontAsk, plan.` | Installed `claude` is **too old** for the `--permission-mode auto` flag the daemon spawns (`permission_flags` default in [config.rs](../../crates/lazybones-engine/src/config.rs)). `auto` exists only on claude ≳2.1.185 | **Update claude** (`claude update`, must run with `CLAUDECODE` unset — it can't update from inside a Claude session) to ≥2.1.185, then restart the daemon. (Alternative: set `permission_flags.claude` to `--permission-mode dontAsk`, the older auto-approve peer — but `auto` on a current binary is the verified path.) |
| 8 | `launch_blocked`; ANSI-stripped headless log shows `Welcome to Claude Code … Let's get started. Choose the text style …` | A claude **update reset onboarding** (`claude update` warns *config install method is 'unknown'*); the first-run **theme picker** parks the headless agent | Seed `~/.claude.json` once: `theme` (e.g. `"dark"`) + `hasCompletedOnboarding: true` (+ `hasCompletedProjectOnboarding: true`). No re-launch of `claude` interactively needed. |
| 9 | `launch_blocked`; headless log shows a **`Settings Warning`** box (`permissions.allow: Invalid permission rule "*" was skipped … ❯ 1. Continue 2. Fix with Claude 3. Exit`) | A newer claude **strict-validates `~/.claude/settings.json`** and interactively prompts on an invalid rule — the bare `"*"` wildcard in `permissions.allow` / `additionalDirectories` is now rejected | Remove the bare `"*"` entries from `permissions.allow` and `additionalDirectories` in `~/.claude/settings.json` (redundant anyway under `--permission-mode auto`/`bypassPermissions`). Keep the literal rules. |
| 10 | `launch_blocked: screen settled before readiness`; headless log shows `Turn browser tools off for future sessions with /chrome. ❯ 1. Yes, use my browser  2. No, keep browser tools off` | claude **auto-detected a Chrome extension** and prompts to enable browser tools on first run — driven by `cachedChromeExtensionInstalled: true` + the growthbook flag `tengu_chrome_auto_enable: true` in `~/.claude.json`. There is **no** env var / `settings.json` key / CLI flag to disable it (confirmed against the docs) | Set `cachedChromeExtensionInstalled: false` in `~/.claude.json` (and `cachedGrowthBookFeatures.tengu_chrome_auto_enable: false` belt-and-suspenders). It can re-set itself if you later run claude interactively with the extension present; the daemon's headless spawns won't re-detect it. |

> **Walls #7–#10 cascade after a `claude` version bump.** A `claude update` can surface them in
> sequence — fix the `auto`-flag gate (#7) and the next spawn parks on the theme picker (#8); seed
> onboarding and the next parks on the settings-wildcard warning (#9). After each fix, **smoke-test
> the spawn in a worktree** before relaunching tasks:
> `env -u CLAUDECODE -u CLAUDE_CODE_SSE_PORT -u CLAUDE_CODE_ENTRYPOINT claude --permission-mode auto -p "reply OK"`
> — a clean `OK`/exit 0 means that wall is cleared. ANSI codes hide the prompt text in headless logs;
> strip them to read the real screen: `sed -r 's/\x1b\[[0-9;?]*[a-zA-Z]//g' <log>`.
| 4 | Looks like a 40-min hang, but it's a **build failure**; worktree crate has a relative path-dep to a sibling checkout outside the repo | Inside `.lazy/wt/<id>/`, the relative path resolves to a non-existent `.lazy/wt/<sibling>` | Symlink the sibling at the wt **parent**: `ln -s /real/path <repo>/.lazy/wt/<sibling>` (`.lazy` is gitignored). Covers every task in the chain. |
| 5 | Daemon (or **management agent**) itself parks at consent/trust; it runs in `.lazy/agent`, not a worktree | Same gates as #1–#3 but on the non-worktree scratch dir | `management/runner.rs` bootstraps `.lazy/agent` with a `.claude/settings.json` allow-list, filters out `--dangerously-skip-permissions`, and `hcom/spawn.rs` scrubs inherited `CLAUDECODE`/`CLAUDE_CODE_*`. Never re-add the skip flag to the management config. |
| 6 | Task lands red instantly with `cargo … --workspace` failing *"could not find `Cargo.toml`"* / *"is not a workspace"* — before any test runs; every retry re-hits it | **Gate inapplicable to the repo**: the default `cargo test --workspace` gate is pointed at a repo whose crates are independent (no root `[workspace]` table — e.g. `rbx-server` + `app-server` side by side) | **Auto-fixed at gate time, against the worktree.** Just before running the gate, a preflight ([scheduler/gate_preflight.rs](../../crates/lazybones-engine/src/scheduler/gate_preflight.rs)) checks the *worktree* (not the base repo): if the agent has made it a real workspace (e.g. a foundation task that wrote a root `[workspace]` Cargo.toml), `--workspace` is left as-is; otherwise the gate is rewritten to per-crate `cargo … --manifest-path <crate>/Cargo.toml` against the crates that actually exist in the worktree. Checking the worktree (post-agent) is deliberate — it avoids fighting the task and avoids targeting phantom or not-checked-out submodule crates. If no crates exist it blocks with a `gate-config` follow-up (set the right gate via `PATCH /workflows/:id`). Needs the daemon rebuilt/restarted to take effect. |

The daemon files a `consent`-kind **follow-up** when it detects walls #1/#3
([scheduler/follow_up.rs](../../crates/lazybones-engine/src/scheduler/follow_up.rs)),
so check the follow-ups surface too.

Note #4 can stack on #1: a task can clear the consent screen and *then* hit the
build seam, so a single "stuck" task may have two causes.

> **A launch-wedged agent now self-surfaces — it should never sit silently `running`.**
> Previously a `launch_blocked` agent (any of #1, #8, #9, #10) left its task `running`
> indefinitely: hcom reports the parked agent with status `blocked`, not `dead`, so the
> stale-reclaim check counted it as a *live, healthy* agent and never acted — an operator
> had to notice and stop it by hand. Fixed: every reclaim tick (~2s) now detects a
> launch-wedged agent (`reclaim::launch_block_reason` — status `blocked`, or a
> `launch_blocked`/`launch_failed`/`screen settled`/`exited before startup` detail) and
> immediately **kills the parked agent, blocks the task with the reason, and files the
> `consent` follow-up** (and auto-retries if a policy is armed). So a host-side launch
> wall shows up as a **blocked task + a follow-up within seconds**, not a phantom
> `running`. If you're on a daemon that predates this, that's why a wall looked invisible
> — rebuild + restart. ([scheduler/reclaim.rs](../../crates/lazybones-engine/src/scheduler/reclaim.rs))

## API caveats while debugging

- **`PATCH /tasks/:id` 401 for author tokens.** Editing an existing task is
  effectively an operator action. An author can only set `auto_retry` at
  *creation* (`POST /workflows/:id/tasks`); to change it afterward, use the
  operator retry-policy control.
- **External (issue-driven) completions are commit-less.** A task can land `done`
  with `commit=null` via `Transition::ExternalDone` when its linked GitHub issue is
  closed — that is not a stall and not lost work.

## Worked example — the `doc-writer` "it's happened again" scare

Live snapshot (`Document writer + asset server + standalone branding`, 5 tasks,
shared branch `lazy/doc-writer`):

```
workflow doc-writer  state=running  task_count=5  done_count=0  started=01:35:56Z
dw-store     running  commit=None  session=kobe  auto_retry=None   deps=[]
dw-api       pending                              deps=[dw-store]
render-spike pending                              deps=[dw-api]
render       pending                              deps=[render-spike]
ui           pending                              deps=[render]
```

`done_count: 0` after ~11 min looked like a stall. It wasn't:

```
$ hcom list
▶ dw-store-kobe [headless]   6s ago: active: Write     # ← WORKING, not parked
$ cd .lazy/wt/doc-writer && git status --short
?? crates/lazybones-store/src/asset/                   # ← real work in progress
?? crates/lazybones-store/src/document/
```

The agent was actively writing the Phase-1 store layer; the other four tasks were
correctly `pending` behind the sequential head. **Verdict: wait, do not retry.**

The one thing worth flagging: `dw-store` has `auto_retry: null`, so if it later
parks after DONE there's no self-heal — that head task is the one to watch, and a
retry policy would harden it.
