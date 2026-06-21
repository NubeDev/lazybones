//! A follow-up: a durable "a human needs to look at this" note, keyed to a run
//! (and optionally a task). The brain's record of work it *cannot* finish on its
//! own — an agent parked on an interactive consent screen, a missing credential,
//! a repeated spawn failure — so the scheduler can stop looping and surface it
//! instead of failing silently.
//!
//! Modelled on the [`event`](crate::Event) run log: a SurrealDB row (auto-minted
//! ULID key, exactly like the event/hcom_log rows) plus a wire projection that
//! leaks no SurrealDB types. Unlike an event (one row per transition,
//! append-only) a follow-up is *upserted* on `(run, dedup_key)` and carries an
//! `open`/`resolved` lifecycle, so re-detecting the same wall bumps the existing
//! note's `seen` count rather than spawning duplicates.

use surrealdb::types::{Datetime, RecordId, RecordIdKey, SurrealValue, ToSql};

/// The table follow-ups live in.
pub(crate) const FOLLOW_UP_TABLE: &str = "follow_up";

/// One actionable note awaiting a human.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct FollowUpRow {
    pub(crate) id: RecordId,
    pub(crate) run: String,
    pub(crate) task: Option<String>,
    /// Stable per-run idempotency key — re-filing the same `(run, dedup_key)`
    /// bumps the existing row instead of inserting a duplicate.
    pub(crate) dedup_key: String,
    /// Coarse class: `consent` | `credential` | `spawn` | `gate` | `note`.
    pub(crate) kind: String,
    /// One-line human-readable summary.
    pub(crate) title: String,
    /// The full reason + suggested fix (markdown, AI- and human-readable).
    pub(crate) detail: String,
    /// Who filed it: `scheduler`, or an agent session name.
    pub(crate) actor: String,
    /// `open` until a human (or the resolving agent) clears it.
    pub(crate) status: String,
    /// How many times this exact follow-up has been (re-)filed — a stuck loop's
    /// pressure gauge, bumped on each idempotent re-file.
    pub(crate) seen: u32,
    pub(crate) created_at: Datetime,
    pub(crate) updated_at: Datetime,
    pub(crate) resolved_at: Option<Datetime>,
}

/// The wire/JSON projection (no SurrealDB types leak out), exactly like
/// [`Event`](crate::Event).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FollowUp {
    /// Opaque row id (the SurrealDB ULID key) — the resolve handle.
    pub id: String,
    /// The run this follow-up belongs to.
    pub run: String,
    /// The task it concerns, when it maps to one.
    pub task: Option<String>,
    /// Coarse class: `consent` | `credential` | `spawn` | `gate` | `note`.
    pub kind: String,
    /// One-line summary.
    pub title: String,
    /// Full reason + suggested fix (markdown).
    pub detail: String,
    /// Who filed it.
    pub actor: String,
    /// `open` or `resolved`.
    pub status: String,
    /// Times this exact follow-up was (re-)filed.
    pub seen: u32,
    /// RFC3339 first-seen timestamp.
    pub created_at: String,
    /// RFC3339 most-recent-update timestamp.
    pub updated_at: String,
    /// RFC3339 resolution timestamp, if resolved.
    pub resolved_at: Option<String>,
}

impl FollowUpRow {
    /// Project to the wire [`FollowUp`]. The bare key string identifies the row
    /// for resolve; `dedup_key` is an internal concern and stays out of the wire.
    pub(crate) fn into_follow_up(self) -> FollowUp {
        FollowUp {
            id: key_string(&self.id),
            run: self.run,
            task: self.task,
            kind: self.kind,
            title: self.title,
            detail: self.detail,
            actor: self.actor,
            status: self.status,
            seen: self.seen,
            created_at: self.created_at.to_string(),
            updated_at: self.updated_at.to_string(),
            resolved_at: self.resolved_at.map(|d| d.to_string()),
        }
    }
}

/// The bare key string of a record id (the auto-minted ULID), mirroring how
/// [`run`](crate::run) projects its own key — `RecordId::new(table, key)`
/// reconstructs the same id for resolve.
fn key_string(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(s) => s.clone(),
        other => other.to_sql(),
    }
}
