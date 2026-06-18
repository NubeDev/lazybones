# The scheduler + the hcom client (the execution plane, in Rust)

> Status: spec, ready to build. This is the #1 priority — nothing runs until it
> exists.
> Audience: whoever implements the loop. Read [vision.md](vision.md) first for the
> "why Rust, not a shell script" framing.

The loop is a **Tokio task inside `lazybonesd`**, not a script under
`~/.hcom/scripts/`. It reads ready Tasks from its own store, provisions worktrees,
spawns agents by **invoking the `hcom` CLI**, gates the result, and advances state.
hcom owns agent process spawning; everything else is ours, in Rust.

## Crate layout

A new crate `lazybones-engine`, depended on by `lazybones-cli`. Verb-per-file,
≤400 lines each ([SCOPE.md](../SCOPE.md) principle 8).

```
crates/lazybones-engine/
  Cargo.toml                 # deps: lazybones-store, tokio, anyhow, serde_json, tracing
  src/
    lib.rs                   # barrel: pub use scheduler::run, config::EngineConfig
    config.rs                # EngineConfig: gate/concurrency/worktree/agent keys
    hcom/
      mod.rs                 # barrel + Hcom client struct
      spawn.rs               # hcom <n> <tool> --tag … --headless … ; parse "Names:"
      events.rs              # hcom events --wait --json --sql …  (block on a condition)
      control.rs             # hcom list --json | kill | fork | resume
    scheduler/
      mod.rs                 # barrel
      run.rs                 # the supervisor loop: tick forever until shutdown
      tick.rs                # one pass: reconcile → promote → claim → spawn
      worktree.rs            # provision per WorktreeMode; teardown on merge
      gate.rs                # run the gate commands in a worktree
      merge.rs               # fast-forward | merge | pr on green
      prompt.rs              # compose the agent prompt from the task spec + charter
      reclaim.rs             # reconcile running tasks against `hcom list`
```

## Config the daemon must now load

`configure.rs` currently loads **only** boot keys and explicitly punts the rest to
"the hcom loop script". That comment is now wrong — the scheduler is the daemon, so
the daemon must load these. Add an `EngineConfig` (parsed from the same
`lazybones.yaml`, same `LAZYBONES_*` env overrides) and pass it into `serve`:

```rust
pub struct EngineConfig {
    pub target_repo: PathBuf,     // LAZYBONES_TARGET_REPO
    pub base_branch: String,      // LAZYBONES_BASE_BRANCH   (default "main")
    pub remote: String,           // LAZYBONES_REMOTE        (default "origin")
    pub gate: Vec<String>,        // LAZYBONES_GATE          (newline-separated in env)
    pub concurrency: usize,       // LAZYBONES_CONCURRENCY   (default 3)
    pub worktrees: bool,          // LAZYBONES_WORKTREES     (default true)
    pub worktree_root: String,    // LAZYBONES_WORKTREE_ROOT (default ".lazy/wt")
    pub branch_prefix: String,    // LAZYBONES_BRANCH_PREFIX (default "lazy/")
    pub merge: MergeMode,         // LAZYBONES_MERGE         fast-forward | merge | pr
    pub agent_tool: String,       // LAZYBONES_AGENT_TOOL    (default "claude")
    pub stale_after_secs: u64,    // LAZYBONES_STALE_AFTER_SECS (default 300)
    pub tick_secs: u64,           // LAZYBONES_TICK_SECS     (default 2)
}
```

`worktrees: false` forces serial execution on `base_branch` (degraded fallback).

## Wiring into the daemon

`serve()` spawns the scheduler next to axum and lets a shutdown signal stop both:

```rust
pub async fn serve(config: Config, engine: EngineConfig) -> anyhow::Result<()> {
    let store = open_store(&config).await?;
    let state = AppState::new(store.clone(), config.run.clone(), config.loop_token.clone());

    let sched = tokio::spawn(lazybones_engine::run(store, engine, config.run.clone()));

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;

    sched.abort();          // stop the loop when the API stops
    Ok(())
}
```

The scheduler shares the **same `StoreHandle`** — it reads/writes the store
in-process, no HTTP round-trip to itself.

## The hcom client (`src/hcom/`)

A typed wrapper over the `hcom` binary (verified against **hcom 0.7.21**). Every
method shells out with `tokio::process::Command` and parses `--json` where
available. Resolve the binary via `$HCOM_BIN` or `"hcom"` on `PATH`.

```rust
pub struct Hcom { bin: String, dir: Option<PathBuf>, env: Vec<(String, String)> }

impl Hcom {
    /// hcom 1 <tool> --tag <tag> --dir <dir> --go --headless --hcom-prompt <prompt>
    /// → parse the "Names: <name>" line; return the agent name (the kill handle).
    pub async fn spawn(&self, tool: &str, tag: &str, dir: &Path, prompt: &str)
        -> anyhow::Result<String>;

    /// hcom events --wait <secs> --json --sql "<expr>"
    /// → block until a matching event or timeout; return the events (possibly empty).
    pub async fn wait(&self, sql: &str, timeout: Duration)
        -> anyhow::Result<Vec<HcomEvent>>;

    /// hcom list --json  → which agents are actually alive (for reclaim).
    pub async fn list(&self) -> anyhow::Result<Vec<HcomAgent>>;

    /// hcom kill tag:<tag> --go   (cancel) / hcom stop <name> (graceful)
    pub async fn kill_tag(&self, tag: &str) -> anyhow::Result<()>;
}
```

Confirmed CLI surface (source-checked in `bin/hcom`): launch flags
`--tag/--dir/--headless/--go/--hcom-prompt/--name`; `hcom events --wait --json
--sql`; `hcom list --json`; `hcom kill tag:<tag>`. Inject agent CLI credentials by
exporting `store.secret_env()` pairs into the spawned process's environment.

## The tick (one pass — `src/scheduler/tick.rs`)

Run every `tick_secs` (and, later, also when the live bus signals a transition):

```text
1. RECONCILE   reclaim.rs: for each task in {running, gating}, if no live hcom
               agent carries its tag AND its heartbeat is older than
               stale_after_secs → transition Reclaim (running→ready). Its worktree
               is kept and reused (agent work is idempotent).

2. PROMOTE     store.newly_ready() → for each, transition Ready (pending→ready).
               This is the dependency cascade: a Task becomes ready the tick after
               its last dep reaches done. (Humans may also promote via the UI;
               both paths are fine.)

3. CLAIM       budget = concurrency - count({running, gating}).
               Take up to `budget` tasks from list_tasks(Some(Ready)).
               For each (respecting `owns` overlap when worktrees are off):
                 a. worktree.rs: provision per task.worktree_mode → (path, branch).
                 b. spawn the agent (below), capture the hcom name.
                 c. store.transition(id, Claim { session: name, worktree, branch }).
                    (Provision BEFORE claim so a provisioning failure blocks cleanly
                    without a half-claimed task.)

4. (async per claimed task, in its own tokio task)
   AWAIT       hcom.wait("msg_thread='<id>' AND (msg_text CONTAINS 'DONE' OR
               msg_text CONTAINS 'BLOCKED')", agent_timeout).
               DONE  → transition Gate (running→gating), then step 5.
               BLOCKED / timeout-with-no-commit → transition Block { reason }.

5. GATE        store.transition(id, Gate) already done; gate.rs runs each
               EngineConfig.gate command in the worktree, in order.
                 all green → merge.rs (fast-forward|merge|pr) → push →
                             transition Done { commit } → worktree teardown.
                 any red   → transition Block { reason: "<cmd> failed:\n<tail>" };
                             keep the worktree for triage.
```

Liveness note: in v1 the scheduler trusts **hcom** for "is the agent alive?"
(`hcom list`) rather than requiring agent-side heartbeats — the loop is the daemon,
so it can observe hcom directly. Agent `POST /tasks/:id/heartbeat` stays supported
but optional. (This resolves the "is a loop connected?" question entirely.)

## Worktree provisioning (`src/scheduler/worktree.rs`)

Honour the stored `worktree_mode` (the contract from
[starting-tasks.md](starting-tasks.md)). All git via `git -C <target_repo> …`.

| mode | action | branch | worktree path |
| --- | --- | --- | --- |
| `New` (default) | `git worktree add <root>/<id> -b <prefix><id> <base>` | `<prefix><id>` | `<target_repo>/<worktree_root>/<id>` |
| `Reuse` | use `task.worktree`; **block** if missing/not a dir | `task.branch` | `task.worktree` |
| `Branch` | `git checkout -B <branch> <base>` in the main checkout; no worktree | `task.branch` or `<prefix><id>` | `<target_repo>` |

If `EngineConfig.worktrees == false`, force `Branch` semantics and serialize
(`concurrency` effectively 1, since one checkout). Teardown after a green merge:
`git worktree remove <path>` (skip for `Branch`).

## The agent prompt (`src/scheduler/prompt.rs`)

Compose from the Task's stored `spec` (the DB is the source — never re-read
`tasks/*.md`) plus a fixed charter. The charter must tell the agent, in order:

1. You are working in `<worktree>` on branch `<branch>`. Implement the task below.
2. Commit your work and `git push <remote> <branch>`.
3. Then signal completion exactly once on the hcom thread named `<task-id>`:
   `hcom send @all --thread <task-id> -- DONE` (or `BLOCKED: <reason>` if you
   cannot finish). Then stop.
4. Do not touch files outside this worktree.

The `--tag <task-id>` on spawn is what makes `hcom kill tag:<task-id>` (cancel) and
thread routing work.

## Cancellation / control surface

Because the Task stores its hcom name (`session`) and is tagged by id, the API can
expose lifecycle control that the scheduler honours (see [vision.md](vision.md) §3):
`POST /tasks/:id/cancel` → `hcom kill tag:<id>` + `transition Block`. Build the
scheduler first; wire these once the loop is green.

## Recovery / restart

No in-memory run state. On boot the first tick's RECONCILE step rebuilds reality
from the store + `hcom list` + `git worktree list`: a `running` task with no live
agent is reclaimed to `ready`; a `gating` task is re-gated. Safe to kill and resume
at any point ([SCOPE.md](../SCOPE.md) "Restart & recovery").

## Test plan

- `config.rs`: env > file > default precedence; gate newline-split from env.
- `hcom/spawn.rs`: parse the `Names:` line from a captured launch fixture.
- `scheduler/worktree.rs`: each mode against a throwaway git repo (tempdir);
  `Reuse` with a missing path blocks.
- `scheduler/gate.rs`: a passing command set → ok; a failing one → reason carries
  the command + output tail.
- `tick.rs` (integration): seed `store → auth` deps; fake hcom binary on `PATH`
  (a shell stub that prints `Names: testagent` and emits a DONE event); assert the
  task walks `pending → ready → running → gating → done` and `auth` only starts
  after `store` is `done`.
