//! The persisted shape of an [`Org`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Org`] carries a
//! plain string id (decisions §3: org-graph ids are not `{org}/{edge}`-namespaced).
//! Optional columns keep the row forward-compatible.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::Org;

/// The table orgs live in.
pub(crate) const ORG_TABLE: &str = "org";

/// SurrealDB-facing org: the reserved `id` thing plus the identity fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct OrgRow {
    pub(crate) id: RecordId,
    pub(crate) title: String,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl OrgRow {
    /// Project a domain [`Org`] into its persisted row.
    pub(crate) fn from_org(o: &Org) -> Self {
        Self {
            id: RecordId::new(ORG_TABLE, o.id.as_str()),
            title: o.title.clone(),
            created_at: Some(o.created_at.clone()),
            updated_at: Some(o.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Org`].
    pub(crate) fn into_org(self) -> Org {
        Org {
            id: org_key(&self.id),
            title: self.title,
            created_at: self.created_at.unwrap_or_default(),
            updated_at: self.updated_at.unwrap_or_default(),
        }
    }
}

/// The raw string form of an org id's key (the part after `org:`).
fn org_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
