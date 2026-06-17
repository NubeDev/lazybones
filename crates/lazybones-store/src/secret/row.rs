//! The persisted shape of a secret at the SurrealDB boundary.
//!
//! Keyed by `tool` (one credential per agent tool). The `blob` column holds the
//! base64 `nonce ‖ ciphertext` from [`Cipher`](super::cipher::Cipher) — the DB
//! never sees plaintext. `env_var`/`hint`/`updated_at` are metadata for listing.

use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, ToSql};

/// The table secrets live in.
pub(crate) const SECRET_TABLE: &str = "secret";

/// SurrealDB-facing secret row: the reserved `id` thing plus the sealed blob.
#[derive(Debug, Clone, PartialEq, SurrealValue)]
pub(crate) struct SecretRow {
    pub(crate) id: RecordId,
    pub(crate) env_var: String,
    /// base64(nonce ‖ ciphertext ‖ tag) — opaque without the master key.
    pub(crate) blob: String,
    pub(crate) hint: String,
    pub(crate) updated_at: String,
}

impl SecretRow {
    /// The tool id (the key part after `secret:`).
    pub(crate) fn tool(&self) -> String {
        match &self.id.key {
            RecordIdKey::String(s) => s.clone(),
            other => other.to_sql(),
        }
    }
}
