//! An hcom-log row — one per raw agent event hcom observed (docs/hcom-logs-scope.md).
//!
//! The **fabric's** record (what hcom saw the agent do/say), as opposed to the
//! [`Event`](crate::Event) run log (the brain's record of lifecycle decisions).
//! `data` is kept verbatim — the message text, status+context, or life
//! action+reason — so nothing the agent emitted is lossily reshaped. The
//! wire/JSON projection ([`HcomLogEntry`]) leaks no SurrealDB types, exactly like
//! [`Event`](crate::Event).

use serde_json::Value;
use surrealdb::types::{Datetime, RecordId, SurrealValue};

/// The table hcom-log rows live in.
pub(crate) const HCOM_LOG_TABLE: &str = "hcom_log";

/// One stored hcom event, keyed to the run/task that owns the agent.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct HcomLogRow {
    pub(crate) id: RecordId,
    /// The workflow this event belongs to (the run-grouping label, like `event.run`).
    pub(crate) run: String,
    /// The task, when the agent's tag maps to one; `None` for run-scoped agents.
    pub(crate) task: Option<String>,
    /// hcom instance name (the transcript handle).
    pub(crate) agent: String,
    /// The hcom `--tag` the agent launched with (task id, or `sup:<run_id>`).
    pub(crate) tag: Option<String>,
    /// hcom's monotonic event id — the ingestion cursor & dedup key.
    pub(crate) hcom_id: i64,
    /// `"message" | "status" | "life"`.
    pub(crate) kind: String,
    /// The raw hcom `data` payload, kept verbatim.
    pub(crate) data: Value,
    /// RFC3339, from hcom's `ts`.
    pub(crate) at: Datetime,
}

/// The wire/JSON projection of an hcom-log entry (no SurrealDB types leak out).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HcomLogEntry {
    /// The workflow this event belongs to.
    pub run: String,
    /// The task whose agent emitted it, when resolvable; `None` for run-scoped.
    pub task: Option<String>,
    /// hcom instance name (the transcript handle).
    pub agent: String,
    /// The hcom `--tag` the agent launched with.
    pub tag: Option<String>,
    /// hcom's monotonic event id.
    pub hcom_id: i64,
    /// `"message" | "status" | "life"`.
    pub kind: String,
    /// The raw hcom `data` payload, verbatim.
    pub data: Value,
    /// RFC3339 timestamp, from hcom's `ts`.
    pub at: String,
}

impl HcomLogRow {
    /// Project to the wire [`HcomLogEntry`].
    pub(crate) fn into_entry(self) -> HcomLogEntry {
        HcomLogEntry {
            run: self.run,
            task: self.task,
            agent: self.agent,
            tag: self.tag,
            hcom_id: self.hcom_id,
            kind: self.kind,
            data: self.data,
            at: self.at.to_string(),
        }
    }
}
