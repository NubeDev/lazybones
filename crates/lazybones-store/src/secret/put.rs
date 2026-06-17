//! Upsert an encrypted secret (the `PUT /secrets/:tool` write).
//!
//! Seals the plaintext with the run's [`Cipher`] and stores only the blob +
//! metadata. Idempotent on `tool`: writing again rotates the value in place.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{Datetime, RecordId};

use crate::error::{Result, StoreError};

use super::cipher::Cipher;
use super::model::{SecretMeta, value_hint};
use super::row::{SECRET_TABLE, SecretRow};

/// Seal and store `value` for `tool` under `env_var`. Returns the new metadata.
///
/// # Errors
/// Returns [`StoreError::Secret`] if encryption fails, or
/// [`StoreError::Operation`] if the write fails.
pub async fn put_secret(
    db: &Surreal<Db>,
    cipher: &Cipher,
    tool: &str,
    env_var: &str,
    value: &str,
) -> Result<SecretMeta> {
    let now = Datetime::now().to_string();
    let row = SecretRow {
        id: RecordId::new(SECRET_TABLE, tool),
        env_var: env_var.to_owned(),
        blob: cipher.seal(value)?,
        hint: value_hint(value),
        updated_at: now.clone(),
    };

    let written: Option<SecretRow> = db
        .upsert((SECRET_TABLE, tool))
        .content(row)
        .await
        .map_err(StoreError::Operation)?;

    written
        .map(|r| SecretMeta {
            tool: r.tool(),
            env_var: r.env_var,
            set: true,
            hint: r.hint,
            updated_at: r.updated_at,
        })
        .ok_or_else(|| StoreError::Secret(format!("secret for {tool} vanished after write")))
}
