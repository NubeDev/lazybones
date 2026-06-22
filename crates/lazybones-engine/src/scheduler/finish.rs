//! Drive one claimed task to a terminal state: await DONE, gate, merge.
//!
//! Spawned in its own tokio task per claim (step 4–5 of the tick). It blocks on
//! the agent's hcom DONE/BLOCKED signal, then re-runs the gate in the worktree
//! and either lands the branch (`done`) or blocks the task, keeping the worktree
//! for triage on a red gate.

use std::time::Duration;

use lazybones_store::{StoreHandle, Task, Transition};

use crate::config::EngineConfig;
use crate::hcom::Hcom;

use super::block::block;
use super::effective::EffectiveGit;
use super::{gate, issue, merge, worktree};

/// The actor recorded on transitions this module drives.
const ACTOR: &str = "scheduler:finish";

/// How long to block on the agent's DONE/BLOCKED signal before treating the task
/// as stalled. Generous: real agent work runs for minutes.
const AWAIT_SECS: u64 = 3600;

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

    match signal {
        Signal::Done => {
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
    Done,
    Blocked(String),
    Timeout,
}

/// Block until the agent posts DONE or BLOCKED on the task's thread, or timeout.
async fn await_signal(hcom: &Hcom, id: &str) -> anyhow::Result<Signal> {
    // hcom routes `--thread <id>` messages; match the thread carrying DONE/BLOCKED.
    let sql = format!(
        "type = 'message' AND json_extract(data, '$.thread') = '{id}' \
         AND (json_extract(data, '$.text') LIKE '%DONE%' \
         OR json_extract(data, '$.text') LIKE '%BLOCKED%')"
    );
    let events = hcom.wait(&sql, Duration::from_secs(AWAIT_SECS)).await?;
    let Some(ev) = events.first() else {
        return Ok(Signal::Timeout);
    };
    let text = ev
        .data
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if text.contains("BLOCKED") {
        Ok(Signal::Blocked(blocked_reason(text)))
    } else {
        Ok(Signal::Done)
    }
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
    // The effective gate is workspace ?? global; an empty list is an explicit
    // no-gate and `gate::run` returns Green over zero commands, landing on DONE.
    let outcome = match gate::run(std::path::Path::new(&worktree_path), &eff.gate).await {
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
            let wt = std::path::Path::new(&worktree_path);
            match merge::commit_worktree(wt, task).await {
                Ok(Some(_sha)) => {} // committed; fall through to land the new head
                Ok(None) => {
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
                    // Clean tree but the agent already committed — land those commits.
                }
                Err(e) => {
                    block(store, task, format!("auto-commit failed: {e}"), ACTOR).await;
                    return;
                }
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

#[cfg(test)]
mod tests {
    use super::blocked_reason;

    #[test]
    fn extracts_reason_after_marker() {
        assert_eq!(blocked_reason("BLOCKED: deps missing"), "deps missing");
        assert_eq!(blocked_reason("foo BLOCKED bar"), "bar");
        assert_eq!(blocked_reason("BLOCKED"), "agent reported BLOCKED");
    }
}
