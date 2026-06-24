//! Drive one claimed task to a terminal state: await DONE, gate, merge.
//!
//! Spawned in its own tokio task per claim (step 4–5 of the tick). It blocks on
//! the agent's hcom DONE/BLOCKED signal, then re-runs the gate in the worktree
//! and either lands the branch (`done`) or blocks the task, keeping the worktree
//! for triage on a red gate.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lazybones_store::{Status, StoreHandle, Task, Transition};

use crate::config::EngineConfig;
use crate::hcom::Hcom;

use super::block::block;
use super::effective::EffectiveGit;
use super::{gate, gate_preflight, issue, merge, worktree};

/// The actor recorded on transitions this module drives.
const ACTOR: &str = "scheduler:finish";

/// The set of task ids this daemon process currently has a drive loop for.
///
/// Owned by the supervisor ([`super::run`]) and shared into every tick. It exists
/// so the per-tick recovery pass ([`super::tick`]) can re-attach a drive loop to an
/// in-flight task that lost its loop on a daemon restart — **without** ever
/// double-driving one it is already running. Inserted before a loop spawns, removed
/// when it ends (even on panic, via [`DriveGuard`]).
pub type Driving = Arc<Mutex<HashSet<String>>>;

/// Removes a task id from the [`Driving`] set when the drive loop ends — including
/// on a panic unwind, so a crashed loop frees its slot for a later recovery pass
/// rather than wedging the task as permanently "being driven".
struct DriveGuard {
    driving: Driving,
    id: String,
}

impl Drop for DriveGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = self.driving.lock() {
            set.remove(&self.id);
        }
    }
}

/// Spawn the drive/resume loop for `task` unless this process is already driving it.
///
/// Claiming a fresh task and recovering an orphaned one both go through here, so the
/// [`Driving`] set is the single guard against two loops racing the same task. The
/// id is reserved (inserted) atomically *before* the loop spawns; if it was already
/// present this is a no-op. Both call sites pass an owned clone of each argument.
pub fn spawn_resume(
    store: StoreHandle,
    hcom: Hcom,
    cfg: EngineConfig,
    eff: EffectiveGit,
    task: Task,
    driving: Driving,
) {
    {
        let mut set = driving.lock().expect("driving set poisoned");
        if !set.insert(task.id.clone()) {
            return; // already being driven by this process
        }
    }
    tokio::spawn(async move {
        let _guard = DriveGuard {
            driving: driving.clone(),
            id: task.id.clone(),
        };
        resume(store, hcom, cfg, eff, task).await;
    });
}

/// Drive `task` to a terminal state, dispatching on its current status so a
/// restart-recovery re-attach resumes from the right point:
/// - `Gating` already passed the agent signal + `Gate` transition, so go straight
///   to gating/landing (re-awaiting would block on a DONE that already fired).
/// - anything else (`Running`) takes the full [`drive`] path: await the agent
///   signal — now liveness-aware, so a parked/exited agent reconciles fast.
async fn resume(
    store: StoreHandle,
    hcom: Hcom,
    cfg: EngineConfig,
    eff: EffectiveGit,
    task: Task,
) {
    if task.status == Status::Gating {
        gate_and_land(&store, &cfg, &eff, &task).await;
    } else {
        drive(store, hcom, cfg, eff, task).await;
    }
}

/// Absolute ceiling on how long to block on the agent's DONE/BLOCKED signal before
/// treating the task as stalled. Generous: real agent work runs for minutes. In
/// practice we almost never reach it — liveness short-circuits first (see
/// [`await_signal`]).
const AWAIT_SECS: u64 = 3600;

/// Each `await_signal` poll blocks this long for a DONE/BLOCKED message, then
/// re-checks the agent's liveness via `hcom list`. Small enough that an exited or
/// parked agent is noticed within a poll, large enough not to hammer hcom.
const POLL_SLICE_SECS: u64 = 20;

/// How long the task's agent may sit *idle* (hcom status not `active`/working) with
/// no DONE/BLOCKED before we treat it as an implicit completion.
///
/// This is the fix for the headless fragility (see project memory
/// `done-transition-reconcile-lag`): a headless `claude` often finishes its work
/// and parks at `listening` **without** posting `DONE`, leaving the task `running`
/// / `commit=null` and wedging the whole dependent chain for the full
/// [`AWAIT_SECS`]. A genuinely working agent flips to `active` on every tool call,
/// so a sustained idle window means it finished (or parked) — we then gate its
/// worktree work rather than wait an hour. Long enough to ignore a brief
/// between-turn pause.
const IDLE_DONE_SECS: u64 = 60;

/// Await the agent's signal for `task`, then gate and finish it.
///
/// Best-effort and self-contained: every failure path ends in a `Block` or a
/// logged error so the supervisor loop never wedges.
pub async fn drive(
    store: StoreHandle,
    hcom: Hcom,
    cfg: EngineConfig,
    eff: EffectiveGit,
    task: Task,
) {
    let signal = match await_signal(&hcom, &task.id).await {
        Ok(s) => s,
        Err(e) => {
            block(&store, &task, format!("await failed: {e}"), ACTOR).await;
            return;
        }
    };

    let inferred = matches!(signal, Signal::AgentDone);
    match signal {
        Signal::Done | Signal::AgentDone => {
            if inferred {
                // The agent exited or went idle without posting DONE — the known
                // headless-parks-at-`listening` fragility. Don't wait the full
                // timeout: gate whatever it committed/left in the worktree.
                tracing::info!(
                    task = %task.id,
                    "agent finished without an explicit DONE (idle/exited); gating its work"
                );
            }
            if let Err(e) = store.transition(&task.id, Transition::Gate, ACTOR).await {
                tracing::warn!(task = %task.id, "gate transition failed: {e}");
                return;
            }
            gate_and_land(&store, &cfg, &eff, &task).await;
        }
        Signal::Blocked(reason) => block(&store, &task, reason, ACTOR).await,
        Signal::Timeout => {
            block(
                &store,
                &task,
                "agent timed out with no DONE signal".to_owned(),
                ACTOR,
            )
            .await;
        }
    }
}

/// What the agent signalled on its hcom thread.
enum Signal {
    /// The agent posted `DONE` on its thread.
    Done,
    /// The agent posted `BLOCKED: <reason>`.
    Blocked(String),
    /// No explicit signal, but the agent exited or went idle (`hcom list`) — the
    /// headless-parks-at-`listening` case. Treated like `Done`: gate its work.
    AgentDone,
    /// Hit the absolute [`AWAIT_SECS`] ceiling with the agent still alive+working.
    Timeout,
}

/// Block until the agent posts DONE/BLOCKED on the task's thread, **or** the agent
/// stops working (exits, or sits idle past [`IDLE_DONE_SECS`]) — whichever comes
/// first.
///
/// The message is the fast, explicit path. The liveness fallback is what makes the
/// scheduler solid against headless agents that finish without posting `DONE`
/// (they park at `listening`): instead of waiting the full [`AWAIT_SECS`] for a
/// signal that never comes, we notice the agent is no longer working and gate its
/// worktree directly. This matches the scheduler's design rule of trusting `hcom`
/// for "is the agent alive?" (docs/scheduler.md, "Liveness note").
async fn await_signal(hcom: &Hcom, id: &str) -> anyhow::Result<Signal> {
    // hcom routes `--thread <id>` messages; match the thread carrying DONE/BLOCKED.
    let sql = format!(
        "type = 'message' AND json_extract(data, '$.thread') = '{id}' \
         AND (json_extract(data, '$.text') LIKE '%DONE%' \
         OR json_extract(data, '$.text') LIKE '%BLOCKED%')"
    );

    let mut waited = 0u64;
    // Consecutive idle seconds observed, and whether we've ever seen the agent
    // alive. `seen` guards the spawn race: a not-yet-registered agent reads as
    // `Gone`, which must NOT be mistaken for an exit before it has even started.
    let mut idle = 0u64;
    let mut seen = false;

    while waited < AWAIT_SECS {
        let slice = POLL_SLICE_SECS.min(AWAIT_SECS - waited);
        let events = hcom.wait(&sql, Duration::from_secs(slice)).await?;
        waited += slice;

        if let Some(ev) = events.first() {
            let text = ev
                .data
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            return Ok(if text.contains("BLOCKED") {
                Signal::Blocked(blocked_reason(text))
            } else {
                Signal::Done
            });
        }

        // No explicit signal this slice — is the agent still working? `hcom list`
        // is authoritative for liveness; a transient list failure is `Unknown` and
        // neither advances nor resets the idle streak.
        match agent_liveness(hcom, id).await {
            Liveness::Working => {
                seen = true;
                idle = 0;
            }
            Liveness::Idle => {
                seen = true;
                idle += slice;
                if idle >= IDLE_DONE_SECS {
                    return Ok(Signal::AgentDone);
                }
            }
            // Gone *after* we've seen it = it exited/crashed → gate its work.
            // Gone *before* = not yet registered; keep waiting.
            Liveness::Gone if seen => return Ok(Signal::AgentDone),
            Liveness::Gone | Liveness::Unknown => {}
        }
    }
    Ok(Signal::Timeout)
}

/// Whether the task's agent is working, idle, gone, or unknown — derived from
/// `hcom list`.
enum Liveness {
    /// hcom reports the agent in a working status (`active`/running/thinking).
    Working,
    /// The agent is present but parked (`listening`/`idle`) — not doing work.
    Idle,
    /// No agent carries this task's tag — it exited or was reaped.
    Gone,
    /// `hcom list` could not be read this poll; make no inference.
    Unknown,
}

/// Classify the task's agent from `hcom list`, matched by its `--tag` (the task id).
async fn agent_liveness(hcom: &Hcom, id: &str) -> Liveness {
    let agents = match hcom.list().await {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!(task = %id, "hcom list failed during await: {e}");
            return Liveness::Unknown;
        }
    };
    match agents.iter().find(|a| a.tag.as_deref() == Some(id)) {
        None => Liveness::Gone,
        Some(a) if is_working(&a.status) => Liveness::Working,
        Some(_) => Liveness::Idle,
    }
}

/// Whether an hcom status string means the agent is actively doing work (a tool
/// call or turn in flight) rather than parked waiting for input. hcom reports
/// `active` while working and `listening`/`idle` when parked; unknown strings are
/// treated as working so we never cut a real agent short on an unfamiliar status.
fn is_working(status: &str) -> bool {
    !matches!(status, "listening" | "idle" | "dead" | "stopped" | "")
}

/// Extract the reason after `BLOCKED:` (or the whole message if none).
fn blocked_reason(text: &str) -> String {
    text.split_once("BLOCKED")
        .map(|(_, rest)| rest.trim_start_matches([':', ' ']).trim().to_owned())
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| "agent reported BLOCKED".to_owned())
}

/// Run the gate; on green, merge + record `done` + teardown; on red, block.
async fn gate_and_land(store: &StoreHandle, cfg: &EngineConfig, eff: &EffectiveGit, task: &Task) {
    let worktree_path = task.worktree.clone().unwrap_or_default();
    let wt = std::path::Path::new(&worktree_path);

    // Preflight the gate against the *worktree* — the agent's work is in it now, so
    // a foundation task that just created a root `[workspace]` Cargo.toml makes the
    // default `cargo … --workspace` gate valid (no rewrite), while a repo that
    // genuinely isn't a workspace gets the gate rewritten to per-crate commands
    // against the crates that actually exist here. Checking the worktree (not the
    // base repo) is what keeps this from fighting the task or targeting phantom /
    // not-checked-out submodule crates. Unfixable → block with a `gate-config`
    // reason instead of running a doomed command.
    let gate_cmds = match gate_preflight::check(wt, &eff.gate) {
        gate_preflight::Preflight::Ok => eff.gate.clone(),
        gate_preflight::Preflight::Fixed { gate, note } => {
            tracing::info!(task = %task.id, "gate preflight auto-fix: {note}");
            gate
        }
        gate_preflight::Preflight::Unfixable { reason } => {
            block(store, task, format!("gate misconfigured: {reason}"), ACTOR).await;
            return;
        }
    };

    // The effective gate is workspace ?? global; an empty list is an explicit
    // no-gate and `gate::run` returns Green over zero commands, landing on DONE.
    let outcome = match gate::run(wt, &gate_cmds).await {
        Ok(o) => o,
        Err(e) => {
            block(store, task, format!("gate could not run: {e}"), ACTOR).await;
            return;
        }
    };

    match outcome {
        gate::GateOutcome::Red(reason) => block(store, task, reason, ACTOR).await,
        gate::GateOutcome::Green => {
            // Auto-commit any uncommitted work BEFORE landing — the agent is not
            // relied on to have run `git commit`. A green gate with a clean tree
            // *and* no commits ahead of base is a real no-op task (the agent did
            // nothing): block it rather than land empty work.
            let progress = workflow_progress(store, task).await;
            match merge::commit_worktree(wt, task, progress).await {
                Ok(Some(_sha)) => {} // committed; fall through to land the new head
                Ok(None) => {
                    // Clean tree, nothing to commit. Two very different cases:
                    //   a) the agent already committed its work → land those commits;
                    //   b) the agent did *nothing* → a no-op task; block it.
                    // In a shared worktree the branch always carries prior tasks'
                    // commits, so `branch_has_commits` (ahead-of-base) can't tell
                    // these apart and wrongly lands (b) with the previous task's sha
                    // — the false-done bug. The right question is "did HEAD move
                    // since *this task* was claimed?": compare against base_commit.
                    let advanced = merge::head_advanced(wt, task.base_commit.as_deref())
                        .await
                        .unwrap_or(true);
                    if !advanced {
                        // base_commit known and HEAD unchanged ⇒ genuinely empty.
                        block(
                            store,
                            task,
                            "task produced no commit of its own (empty task)".to_owned(),
                            ACTOR,
                        )
                        .await;
                        return;
                    }
                    // base_commit absent (legacy/unreadable): fall back to the old
                    // ahead-of-base check so a non-shared empty task is still caught.
                    if task.base_commit.is_none() {
                        let branch = task.branch.clone().unwrap_or_default();
                        let has_commits = merge::branch_has_commits(wt, &eff.base_branch, &branch)
                            .await
                            .unwrap_or(true);
                        if !has_commits {
                            block(
                                store,
                                task,
                                "task produced no changes to commit (empty task)".to_owned(),
                                ACTOR,
                            )
                            .await;
                            return;
                        }
                    }
                    // HEAD advanced (or fallback found commits) — the agent already
                    // committed; land those commits.
                }
                Err(e) => {
                    block(store, task, format!("auto-commit failed: {e}"), ACTOR).await;
                    return;
                }
            }
            // Push the task branch to the remote right after committing, so a
            // finished task's work is backed up before the (mode-specific, possibly
            // slower) landing merge runs. Best-effort: a missing remote is skipped
            // and a push failure is logged, not fatal — `land()` still enforces the
            // authoritative push for the merge.
            if let Some(branch) = task.branch.as_deref()
                && let Err(e) = merge::push_branch(wt, &cfg.remote, branch).await
            {
                tracing::warn!(task = %task.id, "post-commit branch push failed: {e}");
            }
            match merge::land(task, eff, cfg).await {
                Ok(commit) => {
                    match store
                        .transition(&task.id, Transition::Done { commit }, ACTOR)
                        .await
                    {
                        Ok(_) => {
                            // Close-on-done (task → issue): best-effort, never blocks
                            // or reverts the task. `task` already carries the linkage
                            // fields (they're not touched by the Done transition).
                            issue::close_on_done(store, &lazybones_gh::Gh::new(), task).await;
                            if let Err(e) = worktree::teardown(task, eff, cfg).await {
                                tracing::warn!(task = %task.id, "teardown failed: {e}");
                            }
                        }
                        Err(e) => tracing::warn!(task = %task.id, "done transition failed: {e}"),
                    }
                }
                Err(e) => block(store, task, format!("merge failed: {e}"), ACTOR).await,
            }
        }
    }
}

/// `Some((n, total))` for the task's workflow: `total` tasks in the workflow and
/// this one is the `n`-th to complete. `None` for a standalone task (no `run_id`)
/// — there is no workflow to count against, so the commit keeps its plain form.
///
/// The task hasn't transitioned to `Done` yet (that happens after landing), so
/// the ordinal is the count of already-done siblings plus this one.
async fn workflow_progress(store: &StoreHandle, task: &Task) -> Option<(usize, usize)> {
    let run_id = task.run_id.as_deref()?;
    let tasks = store.list_run_tasks(run_id).await.ok()?;
    let total = tasks.len();
    if total == 0 {
        return None;
    }
    let done = tasks.iter().filter(|t| t.status == Status::Done).count();
    Some((done + 1, total))
}

#[cfg(test)]
mod tests {
    use super::{blocked_reason, is_working};

    #[test]
    fn extracts_reason_after_marker() {
        assert_eq!(blocked_reason("BLOCKED: deps missing"), "deps missing");
        assert_eq!(blocked_reason("foo BLOCKED bar"), "bar");
        assert_eq!(blocked_reason("BLOCKED"), "agent reported BLOCKED");
    }

    /// The liveness classifier must read hcom's parked states as *not working* (so
    /// a finished/parked agent triggers the idle-done fallback) while treating any
    /// working or unfamiliar status as working (so a real agent is never cut short).
    #[test]
    fn is_working_distinguishes_parked_from_active() {
        // Parked / terminal statuses → not working (eligible for idle-done).
        for parked in ["listening", "idle", "dead", "stopped", ""] {
            assert!(!is_working(parked), "{parked:?} should read as not working");
        }
        // Working or unknown statuses → working (never cut short).
        for working in ["active", "running", "thinking", "busy", "something-new"] {
            assert!(is_working(working), "{working:?} should read as working");
        }
    }
}
