//! Reconcile `running`/`gating` tasks against `hcom list`.
//!
//! In v1 the scheduler trusts hcom for "is the agent alive?" (the loop is the
//! daemon, so it can observe hcom directly). A task in flight whose tag carries
//! no live agent *and* whose heartbeat is older than `stale_after_secs` is
//! reclaimed to `ready`; its worktree is kept and reused (agent work is
//! idempotent).

use lazybones_store::{Lifecycle, Status, StoreHandle, Task, Transition};

use crate::config::EngineConfig;
use crate::hcom::{Hcom, HcomAgent};

/// The actor recorded on reclaim transitions in the run log.
const ACTOR: &str = "scheduler:reclaim";

/// Reclaim every stale in-flight task, then reap agents that should no longer be
/// alive (issue #05). Best-effort: a single failure is logged and the pass
/// continues. One `hcom list` serves both passes.
pub async fn reconcile(store: &StoreHandle, hcom: &Hcom, cfg: &EngineConfig) {
    let live = match hcom.list().await {
        Ok(agents) => agents,
        // hcom unavailable → assume nothing is reclaimable this tick rather than
        // wrongly reclaiming every in-flight task.
        Err(e) => {
            tracing::warn!("reclaim: hcom list failed, skipping: {e}");
            return;
        }
    };

    for status in [Status::Running, Status::Gating] {
        let tasks = match store.list_tasks(Some(status)).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("reclaim: list {status:?} failed: {e}");
                continue;
            }
        };
        for task in tasks {
            // A *launch-wedged* agent (parked on an unanswerable startup prompt, or
            // exited before it was ready) is NOT a healthy live agent and must not
            // be left silently `running` — its hcom status isn't `dead`, so the
            // stale-reclaim check below would treat it as alive and the task would
            // sit `running` until a human noticed. Surface it immediately: kill the
            // parked agent and `block()` the task, which files the operator
            // follow-up (and auto-retries if a policy is armed). This is the visible
            // "it's blocked, here's why" signal that was missing.
            if let Some(reason) = agent_for(&task.id, &live).and_then(launch_block_reason) {
                tracing::info!(task = %task.id, %reason, "reclaim: agent launch-wedged → blocking");
                let _ = hcom.kill_tag(&task.id).await; // best-effort; reap the parked agent
                super::block::block(store, &task, reason, ACTOR).await;
                continue;
            }
            if !is_reclaimable(&task, &live, cfg.stale_after_secs) {
                continue;
            }
            match store.transition(&task.id, Transition::Reclaim, ACTOR).await {
                Ok(_) => tracing::info!(task = %task.id, "reclaimed stale task to ready"),
                Err(e) => tracing::warn!(task = %task.id, "reclaim transition failed: {e}"),
            }
        }
    }

    reap_finished_agents(store, hcom, &live).await;
}

/// The agent carrying `tag`, if any.
fn agent_for<'a>(tag: &str, live: &'a [HcomAgent]) -> Option<&'a HcomAgent> {
    live.iter().find(|a| a.tag.as_deref() == Some(tag))
}

/// If `agent` is *launch-wedged* — parked on an unanswerable startup prompt, or
/// exited before it reached readiness — return a block reason describing why.
/// `None` for any healthy state (`active`, `idle`/`listening`, or a `dead` agent
/// that crashed mid-work, which the stale-reclaim path correctly retries).
///
/// Two tells, both startup-only so a normal idle/working agent is never matched:
/// - hcom status is `blocked` (a startup park: the screen settled before ready); or
/// - the status/detail names a launch wall — `launch_blocked` / `launch_failed` /
///   `screen settled` / `exited before startup`.
///
/// The returned reason always carries the literal `launch_blocked:` so the
/// follow-up classifier files the operator "needs attention" note (a headless
/// agent parked on an interactive prompt — browser/chrome consent, folder trust,
/// bypass consent, theme picker, …).
fn launch_block_reason(agent: &HcomAgent) -> Option<String> {
    let status = agent.status.to_lowercase();
    let detail = agent.detail.to_lowercase();
    let wedged = status == "blocked"
        || detail.contains("launch_blocked")
        || detail.contains("launch blocked")
        || detail.contains("launch_failed")
        || detail.contains("screen settled")
        || detail.contains("settled before readiness")
        || detail.contains("exited before startup");
    if !wedged {
        return None;
    }
    let what = if agent.detail.trim().is_empty() {
        agent.status.clone()
    } else {
        agent.detail.clone()
    };
    Some(format!("launch_blocked: {what}"))
}

/// Whether `tag` carries a non-dead live agent in `live`.
fn has_live_agent(tag: &str, live: &[HcomAgent]) -> bool {
    live.iter()
        .any(|a| a.tag.as_deref() == Some(tag) && a.status != "dead")
}

/// Reap agents that have outlived their purpose (issue #05 — finished/stopped
/// workflows otherwise leak live agents that accumulate and starve the event
/// drain). Two classes, both keyed off the agent's `tag`:
///
/// 1. **Leaked auto_pr summarizers** — an `<run>-autopr` agent for a run that is
///    stopped, already has its PR recorded, or no longer opts into auto_pr. A
///    legitimate summarizer only lives transiently inside one tick's `open_pr`
///    await (which now reaps it on completion); one still alive here has leaked.
/// 2. **Terminal-task agents** — the agent of a `done`/`blocked` task. Its work is
///    over; the lingering process is pure leak.
///
/// Conservative by construction: it never touches an agent whose task is still
/// in-flight, nor the summarizer of an active run that is still legitimately
/// opening its PR. Best-effort — a kill failure is logged, not fatal.
async fn reap_finished_agents(store: &StoreHandle, hcom: &Hcom, live: &[HcomAgent]) {
    if live.is_empty() {
        return;
    }

    // 1. Leaked auto_pr summarizers.
    let runs = store.list_runs().await.unwrap_or_default();
    for run in &runs {
        let tag = format!("{}-autopr", run.id);
        if !has_live_agent(&tag, live) {
            continue;
        }
        let illegitimate = run.lifecycle == Lifecycle::Stopped
            || run.pr_url.is_some()
            || run.workspace.auto_pr != Some(true);
        if illegitimate {
            match hcom.kill_tag(&tag).await {
                Ok(_) => tracing::info!(run = %run.id, "reap: killed leaked auto_pr summarizer {tag}"),
                Err(e) => tracing::warn!(run = %run.id, "reap: kill {tag} failed: {e}"),
            }
        }
    }

    // 2. Agents of terminal tasks.
    for status in [Status::Done, Status::Blocked] {
        let tasks = match store.list_tasks(Some(status)).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("reap: list {status:?} failed: {e}");
                continue;
            }
        };
        for task in tasks {
            if !has_live_agent(&task.id, live) {
                continue;
            }
            match hcom.kill_tag(&task.id).await {
                Ok(_) => {
                    tracing::info!(task = %task.id, ?status, "reap: killed leaked agent for terminal task")
                }
                Err(e) => tracing::warn!(task = %task.id, "reap: kill failed: {e}"),
            }
        }
    }
}

/// A task is reclaimable when no live agent carries its tag and its heartbeat is
/// stale (or absent).
fn is_reclaimable(task: &Task, live: &[crate::hcom::HcomAgent], stale_after_secs: u64) -> bool {
    let has_live_agent = live
        .iter()
        .any(|a| a.tag.as_deref() == Some(task.id.as_str()) && a.status != "dead");
    if has_live_agent {
        return false;
    }
    heartbeat_age_secs(task).is_none_or(|age| age >= stale_after_secs)
}

/// Seconds since the task's last heartbeat, or `None` if it has none / unparsable
/// (an unparsable or absent heartbeat is treated as stale by the caller).
fn heartbeat_age_secs(task: &Task) -> Option<u64> {
    let hb = task.heartbeat.as_deref()?;
    let then = chrono::DateTime::parse_from_rfc3339(hb).ok()?;
    let age = chrono::Utc::now().signed_duration_since(then.with_timezone(&chrono::Utc));
    u64::try_from(age.num_seconds().max(0)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hcom::HcomAgent;

    fn agent(tag: &str, status: &str) -> HcomAgent {
        HcomAgent {
            name: "a".into(),
            base_name: "a".into(),
            status: status.into(),
            detail: String::new(),
            tag: Some(tag.into()),
        }
    }

    fn agent_d(tag: &str, status: &str, detail: &str) -> HcomAgent {
        HcomAgent {
            name: "a".into(),
            base_name: "a".into(),
            status: status.into(),
            detail: detail.into(),
            tag: Some(tag.into()),
        }
    }

    #[test]
    fn launch_blocked_status_is_detected() {
        // The browser/chrome consent park, theme picker, settings warning, etc. —
        // hcom reports status `blocked`.
        let r = launch_block_reason(&agent_d(
            "t",
            "blocked",
            "launch blocked: screen settled before readiness",
        ))
        .expect("blocked status ⇒ wedged");
        assert!(r.contains("launch_blocked:"), "reason must classify: {r}");
    }

    #[test]
    fn launch_failed_detail_is_detected() {
        // Exit-before-ready (e.g. the invalid `--permission-mode auto` flag on an
        // old claude): status `inactive`, detail names the startup failure.
        let r = launch_block_reason(&agent_d(
            "t",
            "inactive",
            "process exited before startup completed (exit code 1)",
        ))
        .expect("exited-before-startup ⇒ wedged");
        assert!(r.contains("launch_blocked:"), "reason must classify: {r}");
        // But a bare `inactive` with no launch marker is a finished agent, not a wall.
        assert!(launch_block_reason(&agent_d("t", "inactive", "")).is_none());
    }

    #[test]
    fn healthy_agents_are_not_wedged() {
        assert!(launch_block_reason(&agent("t", "active")).is_none());
        assert!(launch_block_reason(&agent("t", "idle")).is_none());
        // A crashed (`dead`) agent is for stale-reclaim to retry, not a launch wall.
        assert!(launch_block_reason(&agent("t", "dead")).is_none());
    }

    #[test]
    fn live_agent_is_not_reclaimed() {
        let task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        assert!(!is_reclaimable(&task, &[agent("auth", "active")], 300));
    }

    #[test]
    fn no_agent_and_no_heartbeat_is_reclaimable() {
        let task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        assert!(is_reclaimable(&task, &[], 300));
    }

    #[test]
    fn dead_agent_is_reclaimable() {
        let task = Task::seed("auth", "r", "t", "s", vec![], vec![], None);
        assert!(is_reclaimable(&task, &[agent("auth", "dead")], 300));
    }
}
