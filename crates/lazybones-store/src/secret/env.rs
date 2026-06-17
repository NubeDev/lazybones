//! Decrypt every stored secret into an env key/value list (the `GET /secrets/env`
//! read — loop only).
//!
//! This is the one read that returns plaintext: the trusted loop fetches it,
//! exports the pairs, and spawns each agent CLI with its credential in the
//! environment. Guarded by the loop token at the API layer; never exposed to an
//! agent session.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::cipher::Cipher;
use super::model::SecretEnv;
use super::row::{SECRET_TABLE, SecretRow};

/// Open all secrets to their `env_var → value` pairs, ordered by var name.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails, or
/// [`StoreError::Secret`] if a blob cannot be decrypted (wrong master key).
pub async fn secret_env(db: &Surreal<Db>, cipher: &Cipher) -> Result<Vec<SecretEnv>> {
    let rows: Vec<SecretRow> = db
        .select(SECRET_TABLE)
        .await
        .map_err(StoreError::Operation)?;

    let mut pairs = Vec::with_capacity(rows.len());
    for r in rows {
        pairs.push(SecretEnv {
            value: cipher.open(&r.blob)?,
            env_var: r.env_var,
        });
    }
    pairs.sort_by(|a, b| a.env_var.cmp(&b.env_var));
    Ok(pairs)
}
