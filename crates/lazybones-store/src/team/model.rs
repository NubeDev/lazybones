//! The durable `Team` document — the mid container of the team graph.
//!
//! A team sits `under` an org and contains projects (`project ->under-> team`). It
//! is cloud-authored and single-writer (decisions §3), so its id stays plain
//! (`team:platform`), never `{org}/{edge}`-namespaced. Membership is on the
//! [`member_of`](super::member) edge; containment on [`under`](super::under).

use serde::{Deserialize, Serialize};

/// A team — sits `under` an org, owns projects, has members.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    /// Friendly, unique id (e.g. `platform`).
    pub id: String,
    /// Human title.
    pub title: String,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Team {
    /// A freshly authored team stamped `created_at == updated_at == now`.
    #[must_use]
    pub fn new(id: impl Into<String>, title: impl Into<String>, now: impl Into<String>) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            title: title.into(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
