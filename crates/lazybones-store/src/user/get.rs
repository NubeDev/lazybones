//! Read a single user by its id.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::User;
use super::row::{USER_TABLE, UserRow};

/// Read `user:<id>`, or `None` if no such user exists.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the read fails.
pub async fn get_user(db: &Surreal<Db>, id: &str) -> Result<Option<User>> {
    let row: Option<UserRow> = db
        .select((USER_TABLE, id.to_owned()))
        .await
        .map_err(StoreError::Operation)?;
    Ok(row.map(UserRow::into_user))
}
