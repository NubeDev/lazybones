//! A scoped session: an identity plus the capabilities it was granted.

use std::collections::HashSet;

use crate::capability::{Capability, ManagementProfile};

/// A session bound to a set of capabilities and, for an agent, its one task.
#[derive(Debug, Clone)]
pub struct ScopedSession {
    actor: String,
    task: Option<String>,
    grants: HashSet<Capability>,
}

impl ScopedSession {
    /// The trusted loop session: every capability, no task binding.
    #[must_use]
    pub fn for_loop(actor: impl Into<String>) -> Self {
        Self {
            actor: actor.into(),
            task: None,
            grants: Capability::loop_grant().iter().copied().collect(),
        }
    }

    /// A management-agent session: a scoped, task-unbound identity whose grant
    /// is derived from its permission profile (`Capability::management_grant`).
    /// Unlike an agent session it is not pinned to one task — it authors across
    /// the queue — but it never holds `Claim` or `Secret`, so it cannot run the
    /// scheduler loop or read secrets (`docs/agent/lazybones-agent-scope.md` §10).
    #[must_use]
    pub fn for_management(actor: impl Into<String>, profile: ManagementProfile) -> Self {
        Self {
            actor: actor.into(),
            task: None,
            grants: Capability::management_grant(profile).iter().copied().collect(),
        }
    }

    /// An agent session bound to a single task id.
    #[must_use]
    pub fn for_agent(actor: impl Into<String>, task: impl Into<String>) -> Self {
        Self {
            actor: actor.into(),
            task: Some(task.into()),
            grants: Capability::agent_grant().iter().copied().collect(),
        }
    }

    /// Who this session acts as (recorded as the event actor).
    #[must_use]
    pub fn actor(&self) -> &str {
        &self.actor
    }

    /// The single task an agent session may drive, if any.
    #[must_use]
    pub fn task(&self) -> Option<&str> {
        self.task.as_deref()
    }

    /// Whether this session holds `cap`.
    #[must_use]
    pub fn can(&self, cap: Capability) -> bool {
        self.grants.contains(&cap)
    }

    /// Whether this session may act on task `id`.
    ///
    /// The loop may act on any task; an agent only on the task it was bound to.
    #[must_use]
    pub fn may_act_on(&self, id: &str) -> bool {
        match &self.task {
            None => true,
            Some(bound) => bound == id,
        }
    }
}
