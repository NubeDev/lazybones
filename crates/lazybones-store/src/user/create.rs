//! Strict creation of a user (authoring; must not clobber).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::User;
use super::row::{USER_TABLE, UserRow};

/// Create `user` as a new record, failing if its id is already in use.
///
/// Returns the stored user as it is after the write.
///
/// # Errors
/// Returns [`StoreError::UserExists`] if a user with that id already exists, or
/// [`StoreError::Operation`] if the read or write fails.
pub async fn create_user(db: &Surreal<Db>, user: &User) -> Result<User> {
    let existing: Option<UserRow> = db
        .select((USER_TABLE, user.id.as_str()))
        .await
        .map_err(StoreError::Operation)?;
    if existing.is_some() {
        return Err(StoreError::UserExists(user.id.clone()));
    }

    let written: Option<UserRow> = db
        .create((USER_TABLE, user.id.as_str()))
        .content(UserRow::from_user(user))
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(UserRow::into_user)
        .ok_or_else(|| StoreError::UserNotFound(user.id.clone()))
}
