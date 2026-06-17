//! Load boot config from `lazybones.yaml`, with `LAZYBONES_*` env overrides.
//!
//! Only the few keys the binary needs to *boot* live here — the bind address,
//! the embedded DB location, namespace/database, the run label, and the loop's
//! bearer token. The rest of `lazybones.yaml` (gate commands, concurrency,
//! worktree toggle) is consumed by the hcom loop script, not the daemon; it is
//! imported into the DB on sync and not re-read here.

use std::path::Path;

use serde::Deserialize;

/// The boot configuration for `lazybonesd`.
#[derive(Debug, Clone)]
pub struct Config {
    /// Socket address the REST API binds to.
    pub bind: String,
    /// Directory the embedded SurrealKV files live in.
    pub data_dir: String,
    /// SurrealDB namespace.
    pub namespace: String,
    /// SurrealDB database.
    pub database: String,
    /// Run label that groups this run's tasks + history.
    pub run: String,
    /// Bearer token the trusted loop authenticates with.
    pub loop_token: String,
    /// Master key the store derives its secret-encryption key from. Never
    /// persisted; protects agent CLI credentials at rest in the `secret` table.
    pub secret_key: String,
}

/// The subset of `lazybones.yaml` the daemon reads, all optional in the file.
#[derive(Debug, Default, Deserialize)]
struct File {
    run: Option<String>,
    api: Option<Api>,
    data_dir: Option<String>,
    namespace: Option<String>,
    database: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Api {
    bind: Option<String>,
}

impl Config {
    /// Load from `path` (if it exists) then apply `LAZYBONES_*` env overrides.
    ///
    /// Missing file is fine — defaults plus env cover a headless boot.
    ///
    /// # Errors
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let file: File = if path.exists() {
            serde_yaml::from_str(&std::fs::read_to_string(path)?)?
        } else {
            File::default()
        };

        Ok(Self {
            bind: env_or(
                "LAZYBONES_BIND",
                file.api.and_then(|a| a.bind),
                "127.0.0.1:7878",
            ),
            data_dir: env_or("LAZYBONES_DATA_DIR", file.data_dir, ".lazy/db"),
            namespace: env_or("LAZYBONES_NAMESPACE", file.namespace, "lazybones"),
            database: env_or("LAZYBONES_DATABASE", file.database, "run"),
            run: env_or("LAZYBONES_RUN", file.run, "lazybones-run"),
            loop_token: std::env::var("LAZYBONES_LOOP_TOKEN")
                .unwrap_or_else(|_| "lazybones-loop".to_owned()),
            // Falls back to the loop token so a fresh local run still has a
            // distinct-enough key; override with LAZYBONES_SECRET_KEY for any
            // real deployment (changing it makes existing secrets undecryptable).
            secret_key: std::env::var("LAZYBONES_SECRET_KEY").unwrap_or_else(|_| {
                std::env::var("LAZYBONES_LOOP_TOKEN")
                    .unwrap_or_else(|_| "lazybones-secret-key".to_owned())
            }),
        })
    }
}

/// Resolve a setting: env var wins, then the file value, then `default`.
fn env_or(var: &str, file: Option<String>, default: &str) -> String {
    std::env::var(var)
        .ok()
        .or(file)
        .unwrap_or_else(|| default.to_owned())
}
