//! The generic attach / detach / list verbs over the `attachment` table.
//!
//! These are deliberately owner-agnostic: callers pass `owner_kind` (e.g.
//! `"template"`) and the polymorphic thing `(thing_kind, thing_id)`. Owner
//! existence is **not** checked here — that's the route layer's job (it 404s a
//! missing owner). The attached thing's existence is **not** enforced at all (no
//! hard FK), because the thing is polymorphic and SurrealDB is SCHEMALESS; a
//! dangling attachment is tolerated. See the module doc on [`super`].

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, SurrealValue};

use crate::error::{Result, StoreError};

use super::row::{ATTACHMENT_TABLE, Attachment, AttachmentRow};

/// The content of a new attachment row (no `id` — SurrealDB mints a ULID key).
#[derive(Debug, Clone, SurrealValue)]
struct NewAttachment {
    owner_kind: String,
    owner_id: String,
    thing_kind: String,
    thing_id: String,
    created_at: Datetime,
}

/// Attach `(thing_kind, thing_id)` to `(owner_kind, owner_id)`. Idempotent: if the
/// exact link already exists it is returned unchanged rather than duplicated (the
/// `attachment_unique` index also guards this at the DB level). Returns the
/// persisted [`Attachment`].
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails.
pub async fn attach(
    db: &Surreal<Db>,
    owner_kind: &str,
    owner_id: &str,
    thing_kind: &str,
    thing_id: &str,
) -> Result<Attachment> {
    if let Some(existing) = find_one(db, owner_kind, owner_id, thing_kind, thing_id).await? {
        return Ok(existing.into_attachment());
    }

    let written: Option<AttachmentRow> = db
        .create(ATTACHMENT_TABLE)
        .content(NewAttachment {
            owner_kind: owner_kind.to_owned(),
            owner_id: owner_id.to_owned(),
            thing_kind: thing_kind.to_owned(),
            thing_id: thing_id.to_owned(),
            created_at: Datetime::now(),
        })
        .await
        .map_err(StoreError::Operation)?;
    written.map(AttachmentRow::into_attachment).ok_or_else(|| {
        StoreError::Operation(surrealdb::Error::thrown(
            "attachment insert returned no row".to_owned(),
        ))
    })
}

/// Detach `(thing_kind, thing_id)` from `(owner_kind, owner_id)`. Returns whether
/// a matching link existed.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn detach(
    db: &Surreal<Db>,
    owner_kind: &str,
    owner_id: &str,
    thing_kind: &str,
    thing_id: &str,
) -> Result<bool> {
    let deleted: Vec<AttachmentRow> = db
        .query(format!(
            "DELETE FROM {ATTACHMENT_TABLE} WHERE owner_kind = $owner_kind \
             AND owner_id = $owner_id AND thing_kind = $thing_kind \
             AND thing_id = $thing_id RETURN BEFORE"
        ))
        .bind(("owner_kind", owner_kind.to_owned()))
        .bind(("owner_id", owner_id.to_owned()))
        .bind(("thing_kind", thing_kind.to_owned()))
        .bind(("thing_id", thing_id.to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(!deleted.is_empty())
}

/// List the attachments of `(owner_kind, owner_id)`, optionally narrowed to one
/// `thing_kind`, newest first.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_attachments(
    db: &Surreal<Db>,
    owner_kind: &str,
    owner_id: &str,
    thing_kind: Option<&str>,
) -> Result<Vec<Attachment>> {
    let mut sql = format!(
        "SELECT * FROM {ATTACHMENT_TABLE} WHERE owner_kind = $owner_kind \
         AND owner_id = $owner_id"
    );
    if thing_kind.is_some() {
        sql.push_str(" AND thing_kind = $thing_kind");
    }
    sql.push_str(" ORDER BY created_at DESC");

    let rows: Vec<AttachmentRow> = db
        .query(sql)
        .bind(("owner_kind", owner_kind.to_owned()))
        .bind(("owner_id", owner_id.to_owned()))
        .bind(("thing_kind", thing_kind.unwrap_or_default().to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(AttachmentRow::into_attachment).collect())
}

/// Fetch the single attachment row matching all four coordinates, if any.
async fn find_one(
    db: &Surreal<Db>,
    owner_kind: &str,
    owner_id: &str,
    thing_kind: &str,
    thing_id: &str,
) -> Result<Option<AttachmentRow>> {
    let rows: Vec<AttachmentRow> = db
        .query(format!(
            "SELECT * FROM {ATTACHMENT_TABLE} WHERE owner_kind = $owner_kind \
             AND owner_id = $owner_id AND thing_kind = $thing_kind \
             AND thing_id = $thing_id LIMIT 1"
        ))
        .bind(("owner_kind", owner_kind.to_owned()))
        .bind(("owner_id", owner_id.to_owned()))
        .bind(("thing_kind", thing_kind.to_owned()))
        .bind(("thing_id", thing_id.to_owned()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().next())
}
