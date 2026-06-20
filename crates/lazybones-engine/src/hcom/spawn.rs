//! `hcom 1 <tool> --tag … --dir … --go --headless --hcom-prompt …` — spawn one
//! headless agent and return its name (the kill handle).
//!
//! hcom prints `Names: <name>` on a launch (src/commands/launch.rs:768); we parse
//! that line. The agent credentials carried on the client are exported into the
//! spawned process so the tool's CLI finds its key.

use std::path::Path;

use super::Hcom;

impl Hcom {
    /// Spawn one headless agent for `tag`, working in `dir`, with `prompt`.
    ///
    /// `model` and `effort`, when set, are forwarded to the tool CLI as
    /// `--model`/`--effort` tool-args (hcom passes through anything it doesn't
    /// recognise). They come from the task's catalog selection; `None` lets the
    /// CLI use its own default.
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
        model: Option<&str>,
        effort: Option<&str>,
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
        if let Some(model) = model {
            cmd.arg("--model").arg(model);
        }
        if let Some(effort) = effort {
            cmd.arg("--effort").arg(effort);
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
