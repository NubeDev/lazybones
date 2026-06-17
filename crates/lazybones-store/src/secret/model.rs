//! The secret domain types: a stored credential's *metadata* (what the UI sees)
//! and the decrypted key/value pair (what the loop exports into an agent's env).
//!
//! The plaintext value is never part of [`SecretMeta`] — listing secrets returns
//! only whether each is set, never the credential itself.

use serde::{Deserialize, Serialize};

/// A stored secret's safe-to-show metadata. No plaintext value, ever.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretMeta {
    /// The agent tool this credential is for (`claude`, `codex`, …).
    pub tool: String,
    /// The environment variable the agent CLI reads (`ANTHROPIC_API_KEY`, …).
    pub env_var: String,
    /// Whether a value is currently stored (always true for a listed secret).
    pub set: bool,
    /// Last 4 chars of the value, for at-a-glance confirmation (`…a1b2`).
    pub hint: String,
    /// RFC3339 timestamp the value was last written.
    pub updated_at: String,
}

/// A decrypted credential as an env key/value — the loop export shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretEnv {
    /// The environment variable name.
    pub env_var: String,
    /// The decrypted value.
    pub value: String,
}

/// Compute the `…last4` hint shown in the UI without leaking the value.
#[must_use]
pub fn value_hint(value: &str) -> String {
    let n = value.chars().count();
    if n <= 4 {
        "•".repeat(n)
    } else {
        format!("…{}", &value[value.len().saturating_sub(4)..])
    }
}
