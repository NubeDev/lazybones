//! Run the gate commands in a worktree, in order; the first failure blocks.
//!
//! A task is `done` only when every gate command exits zero in its worktree
//! (SCOPE.md principle 4). Each command is run through `sh -c` so the configured
//! strings (`cargo test --workspace`) work verbatim.

use std::path::Path;

/// The result of running the gate.
pub enum GateOutcome {
    /// Every command passed.
    Green,
    /// A command failed; the reason carries the command and an output tail.
    Red(String),
}

/// How many trailing characters of a failed command's output to keep in the
/// block reason — enough to triage, short enough to store.
const TAIL_LEN: usize = 2000;

/// Run each `commands` entry in `worktree`, stopping at the first failure.
///
/// # Errors
/// Returns an error only if a command cannot be launched at all.
pub async fn run(worktree: &Path, commands: &[String]) -> anyhow::Result<GateOutcome> {
    for cmd in commands {
        let out = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(worktree)
            .output()
            .await?;
        if !out.status.success() {
            let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
            combined.push_str(&String::from_utf8_lossy(&out.stderr));
            let tail = tail(&combined, TAIL_LEN);
            return Ok(GateOutcome::Red(format!("`{cmd}` failed:\n{tail}")));
        }
    }
    Ok(GateOutcome::Green)
}

/// The last `n` characters of `s`, on a char boundary.
fn tail(s: &str, n: usize) -> &str {
    if s.len() <= n {
        return s;
    }
    let mut start = s.len() - n;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn all_green_passes() {
        let dir = tempfile::tempdir().unwrap();
        let out = run(dir.path(), &["true".into(), "echo ok".into()])
            .await
            .unwrap();
        assert!(matches!(out, GateOutcome::Green));
    }

    #[tokio::test]
    async fn first_red_blocks_with_command_and_tail() {
        let dir = tempfile::tempdir().unwrap();
        let out = run(dir.path(), &["echo boom && false".into(), "true".into()])
            .await
            .unwrap();
        match out {
            GateOutcome::Red(reason) => {
                assert!(reason.contains("echo boom && false"));
                assert!(reason.contains("boom"));
            }
            GateOutcome::Green => panic!("expected red"),
        }
    }
}
