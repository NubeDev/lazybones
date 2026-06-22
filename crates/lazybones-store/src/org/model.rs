//! The durable `Org` record — the root of the team graph (`team -> org`).
//!
//! An org is a cloud-only containment node: the top of the
//! `task → workflow → project → team → org` chain the `under`/authz traversal
//! walks. It is authored once on the cloud plane (single writer), so it keeps a
//! plain id — the `{org}/{edge}` namespacing rule (D4) applies only to syncable,
//! edge-minted rows, never to these cloud-authored graph nodes.

use serde::{Deserialize, Serialize};

/// An organisation, unique install-wide by `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Org {
    /// Friendly, unique id (e.g. `acme`).
    pub id: String,
    /// Human name.
    pub name: String,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Org {
    /// A freshly authored org stamped `created_at == updated_at == now`.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, now: impl Into<String>) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            name: name.into(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
