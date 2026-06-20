//! Edit an existing agent catalog entry (the operator re-write).
//!
//! Overwrites only the authored fields and bumps `updated_at`; `id` and
//! `created_at` are preserved. `now` is injected so the caller owns time (tests
//! pass a fixed stamp).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::{AgentCatalog, AgentCatalogEdit};
use super::row::{AGENT_TABLE, AgentRow};

/// Overwrite the authored fields of `agent:<id>`, preserving id + `created_at`.
///
/// Returns the updated agent as it is after the write.
///
/// # Errors
/// Returns [`StoreError::AgentNotFound`] if no such agent exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn update_agent(
    db: &Surreal<Db>,
    id: &str,
    edit: AgentCatalogEdit,
    now: impl Into<String>,
) -> Result<AgentCatalog> {
    let existing: Option<AgentRow> = db
        .select((AGENT_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    let mut to_write = existing
        .map(AgentRow::into_agent)
        .ok_or_else(|| StoreError::AgentNotFound(id.to_owned()))?;

    to_write.label = edit.label;
    to_write.env_var = edit.env_var;
    to_write.login_hint = edit.login_hint;
    to_write.models = edit.models;
    to_write.default_model = edit.default_model;
    to_write.efforts = edit.efforts;
    to_write.default_effort = edit.default_effort;
    to_write.updated_at = now.into();

    let written: Option<AgentRow> = db
        .upsert((AGENT_TABLE, id.to_owned()))
        .content(AgentRow::from_agent(&to_write))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(AgentRow::into_agent)
        .ok_or_else(|| StoreError::AgentNotFound(id.to_owned()))
}
