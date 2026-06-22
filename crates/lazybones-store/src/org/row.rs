//! The persisted shape of an [`Org`] at the SurrealDB boundary.
//!
//! SurrealDB owns the reserved `id` as a `RecordId`; the domain [`Org`] carries a
//! plain string id. `Option` columns keep the row forward-compatible: a field
//! added later reads back as `None` on rows written before it existed.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

use super::model::Org;

/// The table orgs live in.
pub(crate) const ORG_TABLE: &str = "org";

/// SurrealDB-facing org: the reserved `id` thing plus the org fields.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct OrgRow {
    pub(crate) id: RecordId,
    /// `Option` columns so rows written before a field existed read back fine.
    pub(crate) name: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl OrgRow {
    /// Project a domain [`Org`] into its persisted row.
    pub(crate) fn from_org(o: &Org) -> Self {
        Self {
            id: RecordId::new(ORG_TABLE, o.id.as_str()),
            name: Some(o.name.clone()),
            created_at: Some(o.created_at.clone()),
            updated_at: Some(o.updated_at.clone()),
        }
    }

    /// Reconstruct the domain [`Org`].
    pub(crate) fn into_org(self) -> Org {
        Org {
            id: org_key(&self.id),
            name: self.name.unwrap_or_default(),
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
