//! `hcom 1 <tool> --tag … --dir … --go --headless --hcom-prompt …` — spawn one
//! headless agent and return its name (the kill handle).
//!
//! hcom prints `Names: <name>` on a launch (src/commands/launch.rs:768); we parse
//! that line. The agent credentials carried on the client are exported into the
//! spawned process so the tool's CLI finds its key.

use std::path::Path;

use super::Hcom;

/// The tool-CLI args a launch forwards beyond the fixed hcom flags — the bits
/// resolved per task/workflow/tool, kept together so `spawn`'s signature stays
/// small (hcom passes anything it doesn't recognise straight to the CLI).
#[derive(Debug, Default, Clone, Copy)]
pub struct AgentLaunch<'a> {
    /// `--model <model>` when set; `None` lets the CLI use its own default.
    pub model: Option<&'a str>,
    /// `--effort <effort>` when set; `None` lets the CLI use its own default.
    pub effort: Option<&'a str>,
    /// Extra flags forwarded verbatim to bypass the CLI's interactive gates
    /// (e.g. `--dangerously-skip-permissions` for `claude`); empty for a tool
    /// with no configured mapping. Without them a headless agent in a never-
    /// trusted worktree freezes on a TUI prompt.
    pub permission_flags: &'a [String],
}

impl Hcom {
    /// Spawn one headless agent for `tag`, working in `dir`, with `prompt`.
    ///
    /// `launch` carries the tool-CLI args (model/effort/permission flags); see
    /// [`AgentLaunch`]. They come from the resolved task/workflow/global config.
    ///
    /// Returns the hcom name parsed from the `Names:` line — the handle the
    /// scheduler stores as the task's `session`.
    ///
    /// # Errors
    /// Returns an error if hcom cannot be launched, exits non-zero, or emits no
    /// `Names:` line.
    pub async fn spawn(
        &self,
        tool: &str,
        tag: &str,
        dir: &Path,
        prompt: &str,
        launch: AgentLaunch<'_>,
    ) -> anyhow::Result<String> {
        let mut cmd = self.command();
        // `1 <tool>` launches a single instance of that tool; `--go` skips the
        // interactive confirmation; `--headless` runs without a live terminal.
        cmd.arg("1")
            .arg(tool)
            .arg("--tag")
            .arg(tag)
            .arg("--dir")
            .arg(dir)
            .arg("--go")
            .arg("--headless")
            .arg("--hcom-prompt")
            .arg(prompt);
        // Forwarded to the tool CLI (hcom passes unrecognised args through).
        if let Some(model) = launch.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(effort) = launch.effort {
            cmd.arg("--effort").arg(effort);
        }
        // Per-tool gate-bypass flags (hcom passes them through to the CLI).
        for flag in launch.permission_flags {
            cmd.arg(flag);
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        let out = cmd.output().await?;
        if !out.status.success() {
            anyhow::bail!(
                "hcom spawn for tag {tag} failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        parse_names(&String::from_utf8_lossy(&out.stdout))
            .ok_or_else(|| anyhow::anyhow!("hcom spawn for tag {tag} printed no `Names:` line"))
    }
}

/// Parse the first agent name from hcom's `Names: <name> [<name> …]` line.
fn parse_names(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("Names:"))
        .and_then(|rest| rest.split_whitespace().next())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::parse_names;

    #[test]
    fn parses_first_name() {
        let out = "some preamble\nNames: testagent other\ntrailing\n";
        assert_eq!(parse_names(out).as_deref(), Some("testagent"));
    }

    #[test]
    fn none_without_names_line() {
        assert_eq!(parse_names("no names here\n"), None);
    }
}
