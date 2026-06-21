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
    /// The agent's full name (the kill/stop handle), e.g. `test-be-lulu`.
    #[serde(default)]
    pub name: String,
    /// The agent's short base name, e.g. `lulu`. This — not `name` — is what the
    /// event stream's `instance` field carries (`hcom events`), so the hcom-log
    /// tail keys its name→tag map on `base_name`. Empty when hcom omits it.
    #[serde(default)]
    pub base_name: String,
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

    /// Post `text` to the hcom thread named `thread` (`hcom send @all --thread
    /// <thread> -- <text>`), addressed to every agent on it.
    ///
    /// This is the operator side of the agent's own DONE/BLOCKED protocol (see
    /// `scheduler::prompt`): the task agent listens on the thread named after its
    /// task id, so a message here reaches a *running* agent live. Wired by `POST
    /// /tasks/:id/chat` via [`crate::send_to_agent`].
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched or exits non-zero.
    pub async fn send(&self, thread: &str, text: &str) -> anyhow::Result<()> {
        let out = self
            .command()
            .arg("send")
            .arg("@all")
            .arg("--thread")
            .arg(thread)
            // `--` ends flag parsing so a message starting with `-` is not read as
            // a flag (the agent prompt uses the same `-- <text>` form).
            .arg("--")
            .arg(text)
            .output()
            .await?;
        if !out.status.success() {
            anyhow::bail!(
                "hcom send --thread {thread} failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    /// Cancel every agent tagged with `tag` (`hcom kill tag:<tag> --go`).
    ///
    /// Wired by the `POST /tasks/:id/cancel` control surface (docs/scheduler.md
    /// "Cancellation") via [`crate::cancel_agent`].
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched or exits non-zero.
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
