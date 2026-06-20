//! Append a raw hcom event to the durable hcom log.
//!
//! Idempotent on `(run, hcom_id)`: re-ingesting the same hcom event is an upsert,
//! not a duplicate (docs/hcom-logs-scope.md — the id-based cursor means we rarely
//! re-pull, but a crash mid-tail could, and this makes that a no-op). A
//! pathological multi-MB message is capped: `data.text` past [`MAX_TEXT_BYTES`] is
//! truncated with a `truncated: true` marker, the full thing still in hcom's
//! transcript.

use serde_json::Value;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, SurrealValue};

use crate::error::{Result, StoreError};

use super::row::{HCOM_LOG_TABLE, HcomLogEntry, HcomLogRow};

/// Cap on the stored `data.text` size; beyond it we truncate and mark the row.
/// The full payload survives in hcom's own transcript.
const MAX_TEXT_BYTES: usize = 64 * 1024;

/// The content of a new hcom-log row (no `id` — SurrealDB mints a ULID key).
#[derive(Debug, Clone, SurrealValue)]
struct NewHcomLog {
    run: String,
    task: Option<String>,
    agent: String,
    tag: Option<String>,
    hcom_id: i64,
    kind: String,
    data: Value,
    at: Datetime,
}

/// The fields an ingestion supplies for one event; the cursor/dedup key is
/// `(run, hcom_id)`.
#[derive(Debug, Clone)]
pub struct NewHcomLogEntry {
    pub run: String,
    pub task: Option<String>,
    pub agent: String,
    pub tag: Option<String>,
    pub hcom_id: i64,
    pub kind: String,
    pub data: Value,
    /// RFC3339 string from hcom's `ts`.
    pub at: String,
}

/// Append `entry` to the hcom log, idempotent on `(run, hcom_id)`. Returns the
/// persisted [`HcomLogEntry`] (for live publication on the bus).
///
/// A second write of the same `(run, hcom_id)` updates the existing row rather
/// than inserting a duplicate, so a crash-mid-tail re-ingest is harmless.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails.
pub async fn append_hcom_log(
    db: &Surreal<Db>,
    entry: NewHcomLogEntry,
) -> Result<HcomLogEntry> {
    // Dedup on (run, hcom_id): if a row already exists, return it unchanged.
    let existing: Vec<HcomLogRow> = db
        .query(format!(
            "SELECT * FROM {HCOM_LOG_TABLE} WHERE run = $run AND hcom_id = $hcom_id LIMIT 1"
        ))
        .bind(("run", entry.run.clone()))
        .bind(("hcom_id", entry.hcom_id))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    if let Some(row) = existing.into_iter().next() {
        return Ok(row.into_entry());
    }

    // hcom's `ts` is RFC3339; an unparseable stamp falls back to now (the row is
    // still keyed by the monotonic `hcom_id`, so ordering is preserved regardless).
    let at = entry.at.parse::<Datetime>().unwrap_or_else(|_| Datetime::now());

    let written: Option<HcomLogRow> = db
        .create(HCOM_LOG_TABLE)
        .content(NewHcomLog {
            run: entry.run,
            task: entry.task,
            agent: entry.agent,
            tag: entry.tag,
            hcom_id: entry.hcom_id,
            kind: entry.kind,
            data: cap_text(entry.data),
            at,
        })
        .await
        .map_err(StoreError::Operation)?;
    written.map(HcomLogRow::into_entry).ok_or_else(|| {
        StoreError::Operation(surrealdb::Error::thrown(
            "hcom_log insert returned no row".to_owned(),
        ))
    })
}

/// Truncate an oversized `data.text` past [`MAX_TEXT_BYTES`], marking the object
/// `truncated: true`. Non-object payloads and short text pass through untouched.
fn cap_text(mut data: Value) -> Value {
    let Some(obj) = data.as_object_mut() else {
        return data;
    };
    let Some(Value::String(text)) = obj.get("text") else {
        return data;
    };
    if text.len() <= MAX_TEXT_BYTES {
        return data;
    }
    // Cut on a char boundary at or below the cap.
    let mut end = MAX_TEXT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = text[..end].to_owned();
    obj.insert("text".to_owned(), Value::String(truncated));
    obj.insert("truncated".to_owned(), Value::Bool(true));
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cap_text_leaves_short_payloads() {
        let v = json!({ "text": "hi", "thread": "x" });
        assert_eq!(cap_text(v.clone()), v);
    }

    #[test]
    fn cap_text_truncates_and_marks() {
        let big = "a".repeat(MAX_TEXT_BYTES + 100);
        let capped = cap_text(json!({ "text": big }));
        let text = capped["text"].as_str().unwrap();
        assert!(text.len() <= MAX_TEXT_BYTES);
        assert_eq!(capped["truncated"], json!(true));
    }

    #[test]
    fn cap_text_ignores_non_object() {
        let v = json!("a status string");
        assert_eq!(cap_text(v.clone()), v);
    }
}
