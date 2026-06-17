//! `hcom list --json` and `hcom kill tag:<tag> --go` — observe and cancel agents.
//!
//! `list --json` prints a single JSON array of instance objects carrying `name`,
//! `status`, and `tag` (src/commands/list.rs). `kill tag:<tag>` cancels every
//! agent tagged with the task id.

use serde::Deserialize;

use super::Hcom;

/// One live agent as reported by `hcom list --json` — the fields reclaim reads.
//
// `name` is the graceful-stop handle the cancel API will use; kept on the type
// for completeness though reclaim only matches on `tag`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct HcomAgent {
    /// The agent's full name (the kill/stop handle).
    #[serde(default)]
    pub name: String,
    /// Liveness/status string hcom computes (`active`, `idle`, `dead`, …).
    #[serde(default)]
    pub status: String,
    /// The `--tag` the agent was launched with — the task id, for us.
    #[serde(default)]
    pub tag: Option<String>,
}

impl Hcom {
    /// List the agents hcom currently knows about.
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched, exits non-zero, or its JSON
    /// cannot be parsed.
    pub async fn list(&self) -> anyhow::Result<Vec<HcomAgent>> {
        let out = self.command().arg("list").arg("--json").output().await?;
        if !out.status.success() {
            anyhow::bail!(
                "hcom list failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(trimmed)?)
    }

    /// Cancel every agent tagged with `tag` (`hcom kill tag:<tag> --go`).
    ///
    /// Wired by the `POST /tasks/:id/cancel` control surface (docs/scheduler.md
    /// "Cancellation"); kept here as the typed handle that route will call.
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched or exits non-zero.
    #[allow(dead_code)] // TODO(scheduler): used once the cancel route lands.
    pub async fn kill_tag(&self, tag: &str) -> anyhow::Result<()> {
        let out = self
            .command()
            .arg("kill")
            .arg(format!("tag:{tag}"))
            .arg("--go")
            .output()
            .await?;
        if !out.status.success() {
            anyhow::bail!(
                "hcom kill tag:{tag} failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }
}
