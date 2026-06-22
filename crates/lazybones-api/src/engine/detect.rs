//! Probe the host for the orchestration engine (hcom) and the agent CLIs.
//!
//! lazybones is the queue + gate; the *engine* (hcom) and the per-task *agent
//! CLIs* (claude, codex, …) are external binaries installed separately. The UI
//! needs to show whether they're present so a user can set the run up — these
//! helpers resolve each binary and run a cheap `--version` probe, never failing
//! the request (a missing tool is data, not an error).
//!
//! Resolution does NOT rely on `$PATH` alone. A daemon launched from a desktop
//! shell, a `.app` bundle, or `make dev` often runs with a minimal `PATH` that
//! omits user-local install dirs (`~/.local/bin`, `~/.cargo/bin`, Homebrew) —
//! the exact place `hcom` and these CLIs land. We search those dirs explicitly
//! so "installed" reflects the host, not the daemon's inherited environment.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

/// Resolve `bin` to an absolute path: first via `$PATH`, then via the common
/// user-local install directories a GUI-launched daemon usually can't see.
pub(crate) fn resolve(bin: &str) -> Option<PathBuf> {
    // 1. Honour an explicit PATH entry if the daemon has one.
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(bin);
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    // 2. Fall back to the dirs CLIs install into regardless of PATH.
    for dir in extra_bin_dirs() {
        let candidate = dir.join(bin);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Whether `bin` is installed anywhere we look.
pub(crate) fn on_path(bin: &str) -> bool {
    resolve(bin).is_some()
}

/// The first line of `<bin> --version`, trimmed, if the binary runs. Invokes the
/// resolved absolute path so a stripped `$PATH` doesn't hide an installed tool.
pub(crate) fn version_of(bin: &str) -> Option<String> {
    let exe = resolve(bin)?;
    let out = Command::new(&exe).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .next()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
}

/// Whether an env var is set and non-empty in the daemon's environment.
pub(crate) fn env_set(var: &str) -> bool {
    std::env::var(var).map(|v| !v.is_empty()).unwrap_or(false)
}

/// Outcome of a live credential probe: did the agent CLI launch + respond?
pub(crate) struct ProbeOutcome {
    /// Whether the probe authenticated and the agent answered.
    pub ok: bool,
    /// A short human-readable explanation (success summary or failure reason).
    pub detail: String,
    /// The agent's own reply to the probe, when we could read it back: the model
    /// id + identity it reported. Proof to the user it's really their agent, not
    /// just that a process launched. `None` if the agent didn't get far enough to
    /// answer (e.g. blocked on a prompt) or its transcript couldn't be read.
    pub reply: Option<String>,
}

/// The sentinel we ask the agent to start its reply with, so we can pick its
/// answer out of the PTY screen (which also holds the banner + prompt echo).
const ANSWER_TAG: &str = "ANSWER::";

/// What we ask the probe agent to answer: its model id and who it is, on one
/// line prefixed with [`ANSWER_TAG`], so the test can echo a concrete identity
/// back to the user.
const PROBE_PROMPT: &str = "Reply with exactly one line and nothing else, starting with ANSWER:: \
     and then `model=<your exact model id>, who=<a short phrase for who you are>`";

/// Actually run the agent through hcom to prove its credential works.
///
/// hcom launches agents asynchronously and tracks each as a "batch"; the real
/// signal of a working credential is whether the launched agent *reaches a live
/// state* (the CLI started, authenticated, and is running) versus dying with a
/// launch/auth failure. So the probe:
///   1. `hcom <tool> --headless --hcom-prompt "<probe>" --go` — launch one agent,
///      with `key_env` exported so it authenticates with the credential to test.
///   2. `hcom events launch <batch> --timeout N` — block until the batch reaches
///      a terminal state; this returns a JSON `LaunchResult` (`ready`/`blocked`/
///      `failed` counts) and an exit code (0 ready, 1 error, 2 timeout/blocked).
///   3. Always `hcom kill <batch>` to tear the probe agent down.
///
/// A reached/ready/blocked agent means the CLI authenticated (a bad key dies as
/// `failed` with an auth error long before it can block on work). `key_env` is
/// `(env_var, value)`, exported only to these children, never logged.
pub(crate) fn probe_agent(
    tool: &str,
    key_env: Option<(&str, &str)>,
    timeout_secs: u64,
) -> ProbeOutcome {
    let Some(hcom) = resolve("hcom") else {
        return ProbeOutcome {
            ok: false,
            detail: "hcom (the engine that launches agents) is not installed".to_owned(),
            reply: None,
        };
    };

    // 1. Launch a single headless agent and capture its batch id.
    let mut launch = Command::new(&hcom);
    launch.args([tool, "--headless", "--hcom-prompt", PROBE_PROMPT, "--go"]);
    if let Some((var, value)) = key_env {
        launch.env(var, value);
    }
    let launch_out = match launch.output() {
        Ok(o) => o,
        Err(e) => {
            return ProbeOutcome {
                ok: false,
                detail: format!("could not launch hcom: {e}"),
                reply: None,
            };
        }
    };
    let launch_text = {
        let mut s = String::from_utf8_lossy(&launch_out.stdout).into_owned();
        s.push_str(&String::from_utf8_lossy(&launch_out.stderr));
        s
    };
    // An immediate error (e.g. binary not on PATH) shows up here, before a batch.
    if launch_text.contains("not installed") || launch_text.contains("not in PATH") {
        let reason = launch_text
            .lines()
            .find(|l| l.contains("not installed") || l.contains("not in PATH"))
            .unwrap_or("agent CLI not runnable");
        return ProbeOutcome {
            ok: false,
            detail: reason.trim().to_owned(),
            reply: None,
        };
    }
    let Some(batch) = parse_batch_id(&launch_text) else {
        return ProbeOutcome {
            ok: false,
            detail: "hcom did not start a launch batch (see daemon logs)".to_owned(),
            reply: None,
        };
    };
    // The agent's name (from the `Names:` line) is how we read its reply back.
    let name = parse_agent_name(&launch_text);

    // 2. Block until the batch settles. On success, read the agent's own reply
    //    back so the user sees a concrete model + identity. 3. Tear it down.
    let mut outcome = wait_for_batch(&hcom, &batch, timeout_secs);
    if outcome.ok
        && let Some(name) = &name
    {
        outcome.reply = read_agent_reply(&hcom, name);
    }
    // Kill by name when we have it (kills the exact agent), else by batch.
    let target = name.as_deref().unwrap_or(&batch);
    let _ = Command::new(&hcom).args(["kill", target]).output();
    outcome
}

/// How long to keep polling the agent's screen for its `ANSWER::` line, and how
/// often. A headless Claude launch authenticates within a couple seconds but
/// takes a few more to emit its first reply, so a tight poll converges fast
/// without blocking the request for long.
const REPLY_POLL_ATTEMPTS: u32 = 12;
const REPLY_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1500);

/// Pull the agent name hcom prints on the `Names: <name>` line at launch. A probe
/// launches exactly one agent, so the first name is the one we want.
fn parse_agent_name(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Names:"))
        .and_then(|names| names.split(',').next())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
}

/// Read the probe agent's reply off its live PTY screen.
///
/// A headless agent's answer doesn't land in `hcom transcript` (that reads the
/// CLI's own session journal, which a `--hcom-prompt` launch never writes where
/// the command looks) nor in the event stream — it's rendered straight onto the
/// agent's terminal. `hcom term <name> --json` dumps that screen as a `lines[]`
/// array, so we poll it until our `ANSWER::`-tagged line appears, then clean it
/// up. Best-effort: if the line never shows (agent slow, screen scrolled), we
/// return `None` and the caller keeps the generic "agent is running" detail.
fn read_agent_reply(hcom: &std::path::Path, name: &str) -> Option<String> {
    for _ in 0..REPLY_POLL_ATTEMPTS {
        if let Some(reply) = read_screen_answer(hcom, name) {
            return Some(reply);
        }
        std::thread::sleep(REPLY_POLL_INTERVAL);
    }
    // One last read after the final sleep.
    read_screen_answer(hcom, name)
}

/// One snapshot of `hcom term <name> --json`, scanned for the `ANSWER::` line.
fn read_screen_answer(hcom: &std::path::Path, name: &str) -> Option<String> {
    let out = Command::new(hcom)
        .args(["term", name, "--json"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let json = serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()?;
    let lines = json.get("lines").and_then(serde_json::Value::as_array)?;
    extract_answer(lines.iter().filter_map(serde_json::Value::as_str))
}

/// Pull the model+identity answer out of the agent's screen lines.
///
/// The screen holds a banner, the echoed prompt, and the reply — all of which
/// mention `ANSWER::` (the prompt tells the agent to use it, so the echo carries
/// it too). The agent's actual reply is the one Claude Code renders with a `●`
/// bullet, whereas the prompt echo is rendered under a `❯` glyph. So we look for
/// a bulleted line bearing the tag; only if none exists do we fall back to any
/// tagged line that doesn't still contain the prompt's `<...>` placeholders. We
/// then strip hcom's `[hcom:<name>]` tag and stitch on a wrapped continuation.
fn extract_answer<'a>(lines: impl Iterator<Item = &'a str>) -> Option<String> {
    let lines: Vec<&str> = lines.collect();
    // The reply is the `●`-bulleted line carrying the tag (the agent's output);
    // the prompt echo renders under `❯`, so the bullet alone disambiguates. We
    // deliberately do NOT fall back to a non-bulleted tagged line: the only other
    // tagged lines are the prompt echo, and the caller polls until the bullet
    // appears, so accepting an echo would just return the instruction verbatim.
    let idx = lines
        .iter()
        .rposition(|l| l.trim_start().starts_with('●') && l.contains(ANSWER_TAG))?;
    // Start at the tag, dropping everything before it (bullet, padding).
    let start = &lines[idx][lines[idx].find(ANSWER_TAG)? + ANSWER_TAG.len()..];
    let mut answer = start.trim().to_owned();
    // A wrapped answer continues on the next line until a blank line or a new
    // UI element (bullet/status glyph). Append one continuation line if present.
    if let Some(next) = lines.get(idx + 1) {
        let next = next.trim();
        if !next.is_empty() && !starts_with_glyph(next) {
            answer.push(' ');
            answer.push_str(next);
        }
    }
    // Guard against a half-rendered echo sneaking through: a real answer carries
    // a concrete `model=` value, never the prompt's `<...>` placeholder.
    if answer.contains('<') && answer.contains('>') {
        return None;
    }
    // Strip the trailing `[hcom:<name>]` routing tag hcom appends.
    if let Some(tag) = answer.rfind("[hcom:") {
        answer.truncate(tag);
    }
    let answer = answer.trim();
    if answer.is_empty() {
        return None;
    }
    Some(answer.chars().take(200).collect())
}

/// Whether a screen line opens with a TUI glyph (bullet/spinner/prompt), marking
/// it as a new element rather than a wrapped continuation of the reply.
fn starts_with_glyph(line: &str) -> bool {
    line.starts_with(['●', '✻', '❯', '─', '│', '╰', '*'])
}

/// Pull the `Batch id: <id>` hcom prints on launch out of its output.
fn parse_batch_id(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Batch id:"))
        .map(|id| id.trim().to_owned())
}

/// Block on `hcom events launch <batch>` and read its JSON `LaunchResult` into a
/// probe outcome. Counts decide it: any `ready`/`blocked` agent authenticated and
/// ran (success); `failed` (or no agent reaching a live state) is the credential
/// failing. The exit code is a fallback when the JSON can't be parsed.
fn wait_for_batch(hcom: &std::path::Path, batch: &str, timeout_secs: u64) -> ProbeOutcome {
    let out = Command::new(hcom)
        .args([
            "events",
            "launch",
            batch,
            "--timeout",
            &timeout_secs.to_string(),
        ])
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            return ProbeOutcome {
                ok: false,
                detail: format!("error waiting on agent: {e}"),
                reply: None,
            };
        }
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let exit_ok = out.status.success();

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text.trim()) {
        let count = |k: &str| json.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0);
        let ready = count("ready");
        let blocked = count("blocked");
        let failed = count("failed");
        // The agent reaching ready/blocked proves the CLI authenticated and ran;
        // a credential failure surfaces as `failed` with an auth error.
        if ready > 0 || blocked > 0 {
            return ProbeOutcome {
                ok: true,
                detail: "authenticated; agent launched and is running".to_owned(),
                reply: None,
            };
        }
        if failed > 0 {
            let why = json
                .get("hint")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("agent failed to launch (check the credential)");
            return ProbeOutcome {
                ok: false,
                detail: first_line(why),
                reply: None,
            };
        }
    }

    // No parseable JSON / no decisive counts: fall back to the exit code.
    if exit_ok {
        ProbeOutcome {
            ok: true,
            detail: "authenticated; agent launched".to_owned(),
            reply: None,
        }
    } else {
        ProbeOutcome {
            ok: false,
            detail: first_line(text.trim()),
            reply: None,
        }
    }
}

/// First non-empty line of `s`, for a compact failure detail.
fn first_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("agent failed to run")
        .to_owned()
}

/// Per-tool install state as hcom sees it, keyed by hcom tool id (`claude`,
/// `copilot`, …). hcom is the engine that actually launches agents, so it's the
/// authority on whether a tool is runnable — it resolves editor-bundled CLIs
/// (the VS Code Copilot shim) and snap-managed installs that a naive PATH probe
/// misses. We shell out to `hcom status --json` and read its `tools` map.
///
/// Fail-soft: any failure (hcom absent, non-zero exit, unparseable output)
/// yields an empty map, which the report treats as "nothing detected" rather
/// than erroring the request. The caller falls back accordingly.
pub(crate) fn hcom_tools() -> HashMap<String, bool> {
    let Some(exe) = resolve("hcom") else {
        return HashMap::new();
    };
    let Ok(out) = Command::new(&exe).args(["status", "--json"]).output() else {
        return HashMap::new();
    };
    if !out.status.success() {
        return HashMap::new();
    }
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return HashMap::new();
    };
    let Some(tools) = json.get("tools").and_then(serde_json::Value::as_object) else {
        return HashMap::new();
    };
    tools
        .iter()
        .map(|(tool, info)| {
            let installed = info
                .get("installed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            (tool.clone(), installed)
        })
        .collect()
}

/// Common user-local + system bin dirs CLIs land in, independent of `$PATH`.
fn extra_bin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/bin")); // pipx / pip --user / many installers
        dirs.push(home.join(".cargo/bin")); // cargo install
        dirs.push(home.join(".bun/bin")); // bun
        dirs.push(home.join(".deno/bin")); // deno
        dirs.push(home.join(".npm-global/bin")); // npm -g (custom prefix)
        dirs.push(home.join("bin")); // ~/bin
    }
    dirs.push(PathBuf::from("/usr/local/bin")); // Homebrew (Intel) / make install
    dirs.push(PathBuf::from("/opt/homebrew/bin")); // Homebrew (Apple Silicon)
    dirs.push(PathBuf::from("/usr/bin"));
    dirs
}

/// Whether `path` is a regular file with an executable bit (or just exists on
/// platforms without unix permissions).
fn is_executable(path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
