//! The durable `User` record — a person in the team graph.
//!
//! A user belongs to teams via the [`member_of`](crate::team) edge (where the
//! per-team `manager`/`member` role lives). The third role — **Admin** — is not a
//! team edge but a global flag on the user itself (`is_admin`), per the phase-1
//! decision record (Q3). Like the rest of the org chart a user is cloud-authored
//! (single writer), so it keeps a plain id (D4 namespacing applies only to
//! syncable, edge-minted rows).

use serde::{Deserialize, Serialize};

/// A person, unique install-wide by `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    /// Friendly, unique id (e.g. `ada`).
    pub id: String,
    /// Human name.
    pub name: String,
    /// The global Admin flag (Q3): admin is install-wide, not per-team.
    #[serde(default)]
    pub is_admin: bool,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl User {
    /// A freshly authored, non-admin user stamped `created_at == updated_at == now`.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, now: impl Into<String>) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            name: name.into(),
            is_admin: false,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Mark this user a global admin (builder style).
    #[must_use]
    pub fn as_admin(mut self) -> Self {
        self.is_admin = true;
        self
    }
}
