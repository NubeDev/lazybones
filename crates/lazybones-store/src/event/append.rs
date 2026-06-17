//! Append a transition event to the run log.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, SurrealValue};

use crate::error::{Result, StoreError};

use super::row::{EVENT_TABLE, EventRow};

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

/// Record that `task` moved `from -> to`, driven by `actor`, in `run`.
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
) -> Result<()> {
    let _: Option<EventRow> = db
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
    Ok(())
}
