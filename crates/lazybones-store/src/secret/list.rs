//! List stored secrets as metadata only (the `GET /secrets` read).
//!
//! Returns which tools have a credential set, with a `…last4` hint and the time
//! written — never the plaintext. This is the read the UI's credentials panel
//! renders, so it must be safe to expose without the master key.

use surrealdb::Surreal;
use surrealdb::engine::local::Db;

use crate::error::{Result, StoreError};

use super::model::SecretMeta;
use super::row::{SECRET_TABLE, SecretRow};

/// All stored secrets as safe metadata, ordered by tool id.
///
/// # Errors
/// Returns [`StoreError::Operation`] if the query fails.
pub async fn list_secrets(db: &Surreal<Db>) -> Result<Vec<SecretMeta>> {
    let rows: Vec<SecretRow> = db
        .select(SECRET_TABLE)
        .await
        .map_err(StoreError::Operation)?;

    let mut metas: Vec<SecretMeta> = rows
        .into_iter()
        .map(|r| SecretMeta {
            tool: r.tool(),
            env_var: r.env_var,
            set: true,
            hint: r.hint,
            updated_at: r.updated_at,
        })
        .collect();
    metas.sort_by(|a, b| a.tool.cmp(&b.tool));
    Ok(metas)
}
