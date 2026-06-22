//! File a durable follow-up when a block reason names a wall only a human can
//! clear. Shared by the two block paths — [`tick`](super::tick) (spawn/provision
//! failures) and [`finish`](super::finish) (agent-reported `BLOCKED`, gate/merge
//! failures) — so every way a task can wedge on a human-actionable cause surfaces
//! the same "needs attention" note.
//!
//! The scheduler does **not** file a follow-up for ordinary work failures (a red
//! gate, a merge conflict): those clear with a retry or a code fix and don't need
//! an operator at the host. Only host/credential/consent-class walls qualify.

use lazybones_store::{NewFollowUpEntry, StoreHandle, Task};

/// Inspect a block `reason` and, when it's human-actionable, file (or bump) a
/// follow-up against `task`'s run. Idempotent on `(run, dedup_key)` — a task that
/// keeps hitting the same wall bumps one note's `seen` gauge. Best-effort: a
/// filing failure is logged and never propagates (the block still happens).
pub async fn file_if_actionable(store: &StoreHandle, task: &Task, reason: &str, actor: &str) {
    let Some((kind, title, detail)) = classify(reason) else {
        return;
    };
    // Key to the workflow run when the task belongs to one; fall back to the
    // event-grouping label for a standalone task.
    let run = task.run_id.clone().unwrap_or_else(|| task.run.clone());
    let entry = NewFollowUpEntry {
        run,
        task: Some(task.id.clone()),
        dedup_key: format!("{kind}:{}", task.id),
        kind: kind.to_owned(),
        title,
        detail,
        actor: actor.to_owned(),
    };
    if let Err(e) = store.file_follow_up(entry).await {
        tracing::warn!(task = %task.id, "file_follow_up failed: {e}");
    }
}

/// Classify a block `reason` into `(kind, title, detail-markdown)`, or `None` when
/// the failure is the agent's own work that a retry/fix can clear without operator
/// action on the host. The detail is written for both a human skimming the
/// Follow-ups tab and an agent reading it back over REST.
fn classify(reason: &str) -> Option<(&'static str, String, String)> {
    let lower = reason.to_lowercase();

    // A headless Claude parked on an interactive startup gate — either the
    // one-time *Bypass Permissions* consent screen or the per-folder *trust*
    // dialog. Either way hcom reaps it as `launch_blocked: screen settled before
    // readiness`, surfacing here as a spawn failure. Both fixes are host-side; a
    // bare retry just re-hits the same wall.
    if lower.contains("screen settled")
        || lower.contains("launch_blocked")
        || lower.contains("bypass permissions")
        || lower.contains("trust this folder")
        || lower.contains("trust dialog")
        || lower.contains("permission prompt")
        || lower.contains("memory-note")
        || lower.contains(".claude/")
        || lower.contains("consent")
    {
        return Some((
            "consent",
            "Agent stuck on a permission/trust prompt".to_owned(),
            format!(
                "A headless agent parked on an interactive prompt nobody can answer — \
                 Claude Code's one-time *Bypass Permissions* consent screen, its \
                 per-folder *\"Yes, I trust this folder\"* trust dialog, or a write \
                 into the protected `.claude/` directory (e.g. an auto-memory note).\n\n\
                 **Block reason:** {reason}\n\n\
                 **Auto-memory / `.claude/` write (the usual mid-run cause):** lazybones \
                 spawns agents with `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` so they don't \
                 try to write the host's memory (a protected-path write no allow-list \
                 can pre-approve). If you see this, the daemon predates that fix — \
                 update and restart it; no host change is needed.\n\n\
                 **Folder trust (fresh worktree):** lazybones auto-seeds the launch \
                 dir's trust flag, controlled by `auto_trust_agent_folder` (on by \
                 default; settable per task). If off, turn it back on — or set \
                 `\"hasTrustDialogAccepted\": true` for the dir under `projects` in \
                 `~/.claude.json`.\n\n\
                 **Bypass-permissions consent (one-time per host):** run `claude` \
                 interactively and choose *Yes, I accept*, or set \
                 `bypassPermissionsModeAccepted: true` in `~/.claude.json`.\n\n\
                 After the relevant fix, resolve this follow-up and retry the task."
            ),
        ));
    }

    // An agent that blocked because it lacks a secret/credential — a human must
    // provide it; the agent can't proceed and re-running won't help.
    if lower.contains("credential")
        || lower.contains("api key")
        || lower.contains("api_key")
        || lower.contains("token")
        || lower.contains("auth")
        || lower.contains("login")
        || lower.contains("secret")
    {
        return Some((
            "credential",
            "Agent is missing a credential".to_owned(),
            format!(
                "The agent blocked because it lacks a credential it needs to finish.\n\n\
                 **Block reason:** {reason}\n\n**Fix:** provide the secret (e.g. via the \
                 secrets store / env), then resolve this follow-up and retry the task."
            ),
        ));
    }

    if lower.contains("spawn failed") {
        return Some((
            "spawn",
            "Agent failed to spawn".to_owned(),
            format!(
                "The orchestrator could not launch an agent for this task.\n\n\
                 **Block reason:** {reason}\n\n**Fix:** check `hcom list` and the daemon \
                 log on the host, resolve the underlying cause, then resolve this \
                 follow-up and retry the task."
            ),
        ));
    }

    if lower.contains("provisioning failed") || lower.contains("worktree") {
        return Some((
            "worktree",
            "Worktree provisioning failed".to_owned(),
            format!(
                "Could not create or reuse the git worktree for this task.\n\n\
                 **Block reason:** {reason}\n\n**Fix:** check the repo path, disk space, \
                 and `git worktree list` on the host; clear the conflict, then resolve \
                 this follow-up and retry."
            ),
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn consent_screen_classifies() {
        let (kind, _, _) =
            classify("agent spawn failed: launch blocked: screen settled before readiness")
                .unwrap();
        assert_eq!(kind, "consent");
    }

    #[test]
    fn protected_memory_write_classifies_as_consent() {
        // The mid-run block: an agent parked on a `.claude/` memory-note write.
        let (kind, _, detail) =
            classify("blocked on prompt: Do you want to create memory-note.md?").unwrap();
        assert_eq!(kind, "consent");
        assert!(detail.contains("CLAUDE_CODE_DISABLE_AUTO_MEMORY"));
    }

    #[test]
    fn missing_credential_classifies() {
        let (kind, _, _) = classify("BLOCKED: need a GITHUB_TOKEN to push").unwrap();
        assert_eq!(kind, "credential");
    }

    #[test]
    fn spawn_failure_classifies() {
        let (kind, _, _) = classify("agent spawn failed: exit status: 2").unwrap();
        // "exit status" carries no credential/consent words → falls to spawn.
        assert_eq!(kind, "spawn");
    }

    #[test]
    fn ordinary_work_failure_does_not_classify() {
        assert!(classify("gate red: cargo test failed (3 failures)").is_none());
        assert!(classify("merge failed: conflict in src/main.rs").is_none());
    }
}
