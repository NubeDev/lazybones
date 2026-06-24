//! The durable `Org` document — the root of the team graph.
//!
//! An org is the top container in the org chart (`team ->under-> org`). It is a
//! cloud-authored, single-writer noun (decisions §3): its id stays plain
//! (`org:nube`), never `{org}/{edge}`-namespaced. Like every other store noun it
//! carries only friendly identity plus creation stamps; containment lives on the
//! [`under`](crate::team) relation, not on a column here.

use serde::{Deserialize, Serialize};

/// An organization — the root container a team sits `under`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Org {
    /// Friendly, unique id (e.g. `nube`).
    pub id: String,
    /// Human title.
    pub title: String,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Org {
    /// A freshly authored org stamped `created_at == updated_at == now`.
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
