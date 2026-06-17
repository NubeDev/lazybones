//! A scoped session: an identity plus the capabilities it was granted.

use std::collections::HashSet;

use crate::capability::Capability;

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
