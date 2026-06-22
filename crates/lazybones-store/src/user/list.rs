//! List all users (`GET /users`).

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::User;
use super::row::{USER_TABLE, UserRow};

/// List every user.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_users(db: &Surreal<Db>) -> Result<Vec<User>> {
    let rows: Vec<UserRow> = db
        .query(format!("SELECT * FROM {USER_TABLE}"))
        .await
        .map_err(StoreError::Operation)?
        .take(0)
        .map_err(StoreError::Operation)?;
    Ok(rows.into_iter().map(UserRow::into_user).collect())
}
