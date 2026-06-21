//! Reconcile `running`/`gating` tasks against `hcom list`.
//!
//! In v1 the scheduler trusts hcom for "is the agent alive?" (the loop is the
//! daemon, so it can observe hcom directly). A task in flight whose tag carries
//! no live agent *and* whose heartbeat is older than `stale_after_secs` is
//! reclaimed to `ready`; its worktree is kept and reused (agent work is
//! idempotent).

use lazybones_store::{Status, StoreHandle, Task, Transition};

use crate::config::EngineConfig;
use crate::hcom::Hcom;

/// The actor recorded on reclaim transitions in the run log.
const ACTOR: &str = "scheduler:reclaim";

/// Reclaim every stale in-flight task. Best-effort: a single failure is logged
/// and the pass continues.
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
            if !is_reclaimable(&task, &live, cfg.stale_after_secs) {
                continue;
            }
            match store.transition(&task.id, Transition::Reclaim, ACTOR).await {
                Ok(_) => tracing::info!(task = %task.id, "reclaimed stale task to ready"),
                Err(e) => tracing::warn!(task = %task.id, "reclaim transition failed: {e}"),
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
            tag: Some(tag.into()),
        }
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
