//! A run-log event row — one per status transition (SCOPE.md: "Run log as rows,
//! not prose"). The structured replacement for the old appended loop log.

use surrealdb::types::{Datetime, RecordId, SurrealValue};

/// The table events live in.
pub(crate) const EVENT_TABLE: &str = "event";

/// One queryable transition: which task, from→to, by whom, when.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct EventRow {
    pub(crate) id: RecordId,
    pub(crate) run: String,
    pub(crate) task: String,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) actor: String,
    pub(crate) at: Datetime,
}

/// The wire/JSON projection of an event (no SurrealDB types leak out).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Event {
    /// The run this event belongs to.
    pub run: String,
    /// The task whose status changed.
    pub task: String,
    /// The status the task left.
    pub from: String,
    /// The status the task reached.
    pub to: String,
    /// Who drove the transition (the loop, an agent session, the gate).
    pub actor: String,
    /// RFC3339 timestamp of the transition.
    pub at: String,
}

impl EventRow {
    /// Project to the wire [`Event`].
    pub(crate) fn into_event(self) -> Event {
        Event {
            run: self.run,
            task: self.task,
            from: self.from,
            to: self.to,
            actor: self.actor,
            at: self.at.to_string(),
        }
    }
}
