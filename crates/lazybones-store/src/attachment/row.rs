//! A generic attachment row: it links an *owner* entity to a polymorphic *thing*.
//!
//! Both ends are identified by `(kind, id)` strings rather than typed foreign
//! keys, so any entity can own and any kind of thing can be attached without a
//! schema change. Today the only use is `owner_kind = "template"`,
//! `thing_kind = "skill"`, but the row deliberately stores nothing skill- or
//! template-specific.
//!
//! Like the [`follow_up`](crate::follow_up) row, the SurrealDB key is an
//! auto-minted ULID; the wire projection leaks no SurrealDB types.

use surrealdb::types::{Datetime, RecordId, RecordIdKey, SurrealValue, ToSql};

/// The table attachments live in.
pub(crate) const ATTACHMENT_TABLE: &str = "attachment";

/// One owner→thing link.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct AttachmentRow {
    pub(crate) id: RecordId,
    /// The owning entity's kind (e.g. `template`).
    pub(crate) owner_kind: String,
    /// The owning entity's id (its friendly key, e.g. `open-pr`).
    pub(crate) owner_id: String,
    /// The attached thing's kind (e.g. `skill`).
    pub(crate) thing_kind: String,
    /// The attached thing's id (its uuid/friendly key, e.g. `code-review-rust`).
    pub(crate) thing_id: String,
    pub(crate) created_at: Datetime,
}

/// The wire/JSON projection of an attachment (no SurrealDB types leak out).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Attachment {
    /// Opaque row id (the SurrealDB ULID key).
    pub id: String,
    /// The owning entity's kind (e.g. `template`).
    pub owner_kind: String,
    /// The owning entity's id.
    pub owner_id: String,
    /// The attached thing's kind (e.g. `skill`).
    pub thing_kind: String,
    /// The attached thing's id.
    pub thing_id: String,
    /// RFC3339 attach timestamp.
    pub created_at: String,
}

impl AttachmentRow {
    /// Project to the wire [`Attachment`].
    pub(crate) fn into_attachment(self) -> Attachment {
        Attachment {
            id: key_string(&self.id),
            owner_kind: self.owner_kind,
            owner_id: self.owner_id,
            thing_kind: self.thing_kind,
            thing_id: self.thing_id,
            created_at: self.created_at.to_string(),
        }
    }
}

/// The bare key string of a record id (the auto-minted ULID).
fn key_string(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
