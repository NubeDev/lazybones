//! The one place a scheduler-driven failure becomes a `blocked` task — and, if
//! the task carries a hands-off auto-retry policy with budget left, is revived in
//! place for another attempt.
//!
//! Why this lives in its own module: a task can fail at two very different points
//! in a tick — at claim/spawn time ([`tick`](super::tick), before an agent ever
//! runs) and after the agent signals ([`finish`](super::finish), on a BLOCKED /
//! red gate / merge failure). Both must behave identically: file a follow-up if
//! the wall needs a human, record the block, then auto-retry if policy allows.
//! Keeping that in one function is the only way the two paths can't drift — the
//! original bug was exactly that drift (the claim/spawn `block` had no auto-retry
//! at all, so a spawn failure silently stayed blocked even with a policy set).

use lazybones_store::{StoreHandle, Task, Transition};

/// Block `task` with `reason`, then auto-retry it if its policy permits.
///
/// Always, in order:
/// 1. File a "needs a human" follow-up when `reason` names an operator-only wall
///    (idempotent — a looping task bumps one note's `seen` gauge, never spams).
/// 2. Record the `* -> blocked` transition under `actor` (carries the reason; a
///    `Revive` is only legal *from* `blocked`, so this must land first).
/// 3. If the now-blocked task has an `auto_retry` strategy and spent budget is
///    below its cap, revive it in place with the strategy's guidance (bumping the
///    spent counter so the cap is enforced).
///
/// Best-effort throughout: any failure leaves the task safely `blocked`, so the
/// supervisor loop never wedges and a task that can't be revived simply waits for
/// a human. The revive is also intentionally tolerant of a *concurrent* operator
/// or `/loop` client that retried the same task first: that flips it out of
/// `blocked`, the `Revive` then fails as illegal, and we log it as a benign race
/// rather than an error (the human/loop action already recovered the task).
pub async fn block(store: &StoreHandle, task: &Task, reason: String, actor: &str) {
    let id = &task.id;

    // 1. Surface the wall to an operator if it's one only a human can clear.
    super::follow_up::file_if_actionable(store, task, &reason, actor).await;

    // 2. Record the block. This is the durable failure; everything after is the
    //    optional hands-off recovery.
    let blocked = match store
        .transition(id, Transition::Block { reason: reason.clone() }, actor)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(task = %id, "block transition failed: {e}");
            return;
        }
    };

    // 3. Auto-retry, if armed and under budget.
    let Some(strategy) = blocked.auto_retry else {
        return; // No policy: stays blocked for an operator.
    };
    if blocked.retry_count >= blocked.max_retries {
        tracing::info!(
            task = %id,
            spent = blocked.retry_count,
            cap = blocked.max_retries,
            "auto-retry budget exhausted; staying blocked for a human"
        );
        return;
    }

    // Revive with the strategy's guidance (and bump the spent counter so the cap
    // is enforced). The worktree is kept, so the re-spawn resumes in place.
    let guidance = strategy.guidance(&reason);
    match store.revive_with_guidance(id, &guidance, actor, true).await {
        Ok(_) => tracing::info!(
            task = %id,
            strategy = strategy.as_str(),
            attempt = blocked.retry_count + 1,
            cap = blocked.max_retries,
            "auto-retry: revived blocked task"
        ),
        // An illegal transition here means the task is no longer `blocked` — an
        // operator or `/loop` client retried/restarted it between our block and
        // this revive. That already recovered it, so this is a benign race, not a
        // failure: leave their action to stand.
        Err(lazybones_store::StoreError::IllegalTransition { .. }) => tracing::info!(
            task = %id,
            "auto-retry skipped: task was already revived by an operator/loop retry"
        ),
        Err(e) => tracing::warn!(task = %id, "auto-retry revive failed (staying blocked): {e}"),
    }
}
