//! A typed client over the `hcom` binary (verified against hcom 0.7.21).
//!
//! Every method shells out with [`tokio::process::Command`] and parses the
//! `--json` output where hcom offers it. The binary resolves via `$HCOM_BIN`,
//! else `hcom` on `PATH`. hcom owns agent process spawning; this is the thin,
//! typed surface the scheduler drives it through.

mod control;
mod events;
mod spawn;

pub use control::HcomAgent;
pub use events::HcomEvent;

use std::path::PathBuf;

/// A handle to the `hcom` CLI for one run.
#[derive(Debug, Clone)]
pub struct Hcom {
    /// The binary to invoke (`$HCOM_BIN` or `"hcom"`).
    bin: String,
    /// `$HCOM_DIR` for the run's state, if pinned (else hcom's default).
    dir: Option<PathBuf>,
    /// Extra environment exported into every spawned agent — the decrypted
    /// agent CLI credentials (`store.secret_env()` pairs).
    env: Vec<(String, String)>,
}

impl Hcom {
    /// Resolve the binary from `$HCOM_BIN` (else `"hcom"`) and the run's state
    /// dir from `$HCOM_DIR` (else hcom's default); no agent env yet.
    #[must_use]
    pub fn discover() -> Self {
        Self {
            bin: std::env::var("HCOM_BIN").unwrap_or_else(|_| "hcom".to_owned()),
            dir: std::env::var_os("HCOM_DIR").map(PathBuf::from),
            env: Vec::new(),
        }
    }

    /// Override the binary path (a test stub, or an explicit install path).
    #[must_use]
    pub fn with_bin(mut self, bin: impl Into<String>) -> Self {
        self.bin = bin.into();
        self
    }

    /// Replace the agent credential env exported on spawn.
    #[must_use]
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    /// A fresh `Command` for `self.bin` with `$HCOM_DIR` applied (but not the
    /// agent credentials — those are injected only on `spawn`).
    fn command(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.bin);
        if let Some(dir) = &self.dir {
            cmd.env("HCOM_DIR", dir);
        }
        cmd
    }
}
