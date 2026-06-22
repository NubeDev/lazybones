//! The durable `User` document — a person in the team graph.
//!
//! A user is cloud-authored and single-writer (decisions §3), so its id stays
//! plain (`user:ada`), never `{org}/{edge}`-namespaced. The global `admin` flag
//! lives here on the user; the per-team `manager`/`member` distinction lives on
//! the [`member_of`](crate::team) edge instead (decisions §2 notes, projects.md
//! "Roles").

use serde::{Deserialize, Serialize};

/// A person — a member of one or more teams (via [`member_of`](crate::team)).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    /// Friendly, unique id (e.g. `ada`).
    pub id: String,
    /// Human display name.
    pub name: String,
    /// Global admin flag — org-wide authority. Per-team manager/member lives on
    /// the `member_of` edge, not here.
    #[serde(default)]
    pub admin: bool,
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
            admin: false,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Mark this user a global admin (builder style).
    #[must_use]
    pub fn as_admin(mut self) -> Self {
        self.admin = true;
        self
    }
}
