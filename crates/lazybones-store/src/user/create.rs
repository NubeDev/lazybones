//! Idempotent creation of a user (cloud-authored, single-writer).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::User;
use super::row::{USER_TABLE, UserRow};

/// Ensure `user:<id>` exists, returning it. Like the rest of the org graph this is
/// cloud-authored and single-writer (decisions §3), so create is idempotent:
/// re-creating an existing id returns the stored record rather than erroring.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read or write fails.
pub async fn create_user(db: &Surreal<Db>, user: &User) -> Result<User> {
    let existing: Option<UserRow> = db
        .select((USER_TABLE, user.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if let Some(row) = existing {
        return Ok(row.into_user());
    }

    let written: Option<UserRow> = db
        .create((USER_TABLE, user.id.as_str()))
        .content(UserRow::from_user(user))
        .await
        .map_err(StoreError::Operation)?;

    Ok(written.map_or_else(|| user.clone(), UserRow::into_user))
}
