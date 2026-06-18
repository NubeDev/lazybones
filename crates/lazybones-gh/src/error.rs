use std::process::ExitStatus;

/// Errors from invoking the `gh` CLI.
#[derive(Debug, thiserror::Error)]
pub enum GhError {
    /// The `gh` binary could not be spawned (not installed / not on PATH).
    #[error("failed to run `{bin}`: {source} (is the GitHub CLI installed and on PATH?)")]
    Spawn {
        bin: String,
        #[source]
        source: std::io::Error,
    },

    /// `gh` ran but exited non-zero. We surface stderr so the caller (and the
    /// user) can see auth prompts, rate limits, "not found", etc.
    #[error("`gh {args}` exited with {status}: {stderr}")]
    Command {
        args: String,
        status: ExitStatus,
        stderr: String,
    },

    /// `gh` succeeded but its JSON output didn't match what we expected.
    #[error("failed to parse `gh` JSON output: {0}")]
    Json(#[from] serde_json::Error),
}
