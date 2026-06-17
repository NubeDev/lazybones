//! Append a transition event to the run log.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, SurrealValue};

use crate::error::{Result, StoreError};

use super::row::{EVENT_TABLE, Event, EventRow};

/// The content of a new event (no `id` — SurrealDB mints a ULID key).
#[derive(Debug, Clone, SurrealValue)]
struct NewEvent {
    run: String,
    task: String,
    from: String,
    to: String,
    actor: String,
    at: Datetime,
}

/// Record that `task` moved `from -> to`, driven by `actor`, in `run`, and
/// return the persisted [`Event`] (for live publication on the event bus).
///
/// # Errors
/// Returns [`StoreError::Operation`] if the write fails.
pub async fn append_event(
    db: &Surreal<Db>,
    run: &str,
    task: &str,
    from: &str,
    to: &str,
    actor: &str,
) -> Result<Event> {
    let written: Option<EventRow> = db
        .create(EVENT_TABLE)
        .content(NewEvent {
            run: run.to_owned(),
            task: task.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
            actor: actor.to_owned(),
            at: Datetime::now(),
        })
        .await
        .map_err(StoreError::Operation)?;
    written
        .map(EventRow::into_event)
        .ok_or_else(|| {
            StoreError::Operation(surrealdb::Error::thrown(
                "event insert returned no row".to_owned(),
            ))
        })
}
