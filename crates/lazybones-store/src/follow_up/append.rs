//! File a follow-up, idempotent on `(run, dedup_key)`.
//!
//! Re-filing the same `(run, dedup_key)` does **not** insert a duplicate: it bumps
//! the existing row's `seen` counter and `updated_at`, and re-opens it if it had
//! been resolved (the wall came back). This is what lets the scheduler call
//! `file_follow_up` every tick it detects a stuck agent without flooding the
//! table — one row per distinct problem, with a pressure gauge.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, SurrealValue};

use crate::error::{Result, StoreError};

use super::row::{FOLLOW_UP_TABLE, FollowUp, FollowUpRow};

/// The content of a new follow-up row (no `id` — SurrealDB mints a ULID key).
#[derive(Debug, Clone, SurrealValue)]
struct NewFollowUp {
    run: String,
    task: Option<String>,
    dedup_key: String,
    kind: String,
    title: String,
    detail: String,
    actor: String,
    status: String,
    seen: u32,
    created_at: Datetime,
    updated_at: Datetime,
    resolved_at: Option<Datetime>,
}

/// The fields a caller supplies to file a follow-up. The idempotency key is
/// `(run, dedup_key)`; pick a `dedup_key` that's stable for "the same problem"
/// (e.g. `consent:<task>` for a per-task consent wall) so re-files coalesce.
#[derive(Debug, Clone)]
pub struct NewFollowUpEntry {
    pub run: String,
    pub task: Option<String>,
    pub dedup_key: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub actor: String,
}

/// File `entry`, idempotent on `(run, dedup_key)`. A fresh problem inserts an
/// `open` row with `seen = 1`; a recurring one bumps `seen`, refreshes
/// `updated_at`, re-opens if resolved, and refreshes the (possibly improved)
/// title/detail. Returns the persisted [`FollowUp`] for live publication.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails.
pub async fn file_follow_up(db: &Surreal<Db>, entry: NewFollowUpEntry) -> Result<FollowUp> {
    let now = Datetime::now();

    // Coalesce on (run, dedup_key).
    let existing: Vec<FollowUpRow> = db
        .query(format!(
            "SELECT * FROM {FOLLOW_UP_TABLE} WHERE run = $run AND dedup_key = $dedup_key LIMIT 1"
        ))
        .bind(("run", entry.run.clone()))
        .bind(("dedup_key", entry.dedup_key.clone()))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;

    if let Some(prev) = existing.into_iter().next() {
        let updated: Option<FollowUpRow> = db
            .query(
                "UPDATE $id SET seen = seen + 1, updated_at = $now, status = 'open', \
                 resolved_at = NONE, title = $title, detail = $detail, actor = $actor \
                 RETURN AFTER",
            )
            .bind(("id", prev.id.clone()))
            .bind(("now", now))
            .bind(("title", entry.title))
            .bind(("detail", entry.detail))
            .bind(("actor", entry.actor))
            .await
            .map_err(StoreError::Operation)?
            .take(0)
            .map_err(StoreError::Operation)?;
        return updated.map(FollowUpRow::into_follow_up).ok_or_else(|| {
            StoreError::Operation(surrealdb::Error::thrown(
                "follow_up update returned no row".to_owned(),
            ))
        });
    }

    let written: Option<FollowUpRow> = db
        .create(FOLLOW_UP_TABLE)
        .content(NewFollowUp {
            run: entry.run,
            task: entry.task,
            dedup_key: entry.dedup_key,
            kind: entry.kind,
            title: entry.title,
            detail: entry.detail,
            actor: entry.actor,
            status: "open".to_owned(),
            seen: 1,
            created_at: now,
            updated_at: now,
            resolved_at: None,
        })
        .await
        .map_err(StoreError::Operation)?;
    written.map(FollowUpRow::into_follow_up).ok_or_else(|| {
        StoreError::Operation(surrealdb::Error::thrown(
            "follow_up insert returned no row".to_owned(),
        ))
    })
}
