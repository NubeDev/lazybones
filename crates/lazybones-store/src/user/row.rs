//! The persisted shape of a [`User`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`User`] carries a
//! plain string id. `is_admin` is always written (the schema declares it
//! `TYPE bool`); the remaining `Option` columns keep the row forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::User;

/// The table users live in.
pub(crate) const USER_TABLE: &str = "user";

/// SurrealDB-facing user: the reserved `id` thing plus the user fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct UserRow {
    pub(crate) id: RecordId,
    /// `Option` columns so rows written before a field existed read back fine.
    pub(crate) name: Option<String>,
    /// The global admin flag; always written (schema declares it `TYPE bool`).
    pub(crate) is_admin: bool,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl UserRow {
    /// Project a domain [`User`] into its persisted row.
    pub(crate) fn from_user(u: &User) -> Self {
        Self {
            id: RecordId::new(USER_TABLE, u.id.as_str()),
            name: Some(u.name.clone()),
            is_admin: u.is_admin,
            created_at: Some(u.created_at.clone()),
            updated_at: Some(u.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`User`].
    pub(crate) fn into_user(self) -> User {
        User {
            id: user_key(&self.id),
            name: self.name.unwrap_or_default(),
            is_admin: self.is_admin,
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of a user id's key (the part after `user:`).
fn user_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
