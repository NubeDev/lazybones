//! Pre-seed Claude Code's per-folder trust flag so a headless agent launched in a
//! never-before-seen directory doesn't freeze on the interactive folder-trust
//! dialog ("Is this a project you created or one you trust? … Yes, I trust this
//! folder"). A headless PTY agent can't answer it, so hcom reaps it
//! `launch_blocked: screen settled before readiness` and the scheduler loops.
//!
//! This is a **distinct gate** from the per-tool approval allow-list the scheduler
//! writes into a worktree's `.claude/settings.json`: that list clears *tool*
//! prompts, the folder-trust dialog is keyed per absolute path under
//! `projects.<path>.hasTrustDialogAccepted` in Claude's `.claude.json`. Only
//! seeding that flag clears the dialog — and unlike `--dangerously-skip-permissions`
//! it doesn't drag in the separate one-time bypass-permissions consent screen.
//!
//! Mirrors the trust-marker preprocessing hcom already does for Cursor/Codex/
//! Gemini/Copilot, which has no equivalent arm for `claude`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Resolve the `.claude.json` Claude Code reads.
///
/// Claude honours `CLAUDE_CONFIG_DIR` (hcom sets it for isolated runs); when set
/// the config is `$CLAUDE_CONFIG_DIR/.claude.json`, else `$HOME/.claude.json`.
/// Resolving the same file the launched CLI reads is what makes the seed take.
/// Returns `None` only when neither var is set (no place to write).
fn claude_config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir).join(".claude.json"));
    }
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(|home| PathBuf::from(home).join(".claude.json"))
}

/// Mark `dir`'s folder trusted for a headless `claude` launch.
///
/// Best-effort and idempotent: a no-op when already trusted, and any failure
/// (no `HOME`, unreadable/garbled config, write error) is logged and swallowed —
/// the launch still proceeds, falling back to whatever gate-clearing the caller's
/// `permission_flags` provide. Only ever touches `projects.<dir>`; every other
/// key in `.claude.json` (including the global `bypassPermissionsModeAccepted`)
/// is preserved verbatim.
pub fn seed_claude_folder_trust(dir: &Path) {
    let Some(path) = claude_config_path() else {
        tracing::warn!(
            dir = %dir.display(),
            "claude folder-trust seed skipped: neither CLAUDE_CONFIG_DIR nor HOME is set"
        );
        return;
    };
    // Canonicalize so the key matches the absolute path Claude records — it keys
    // trust on the resolved working directory, not the (possibly relative) arg.
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let folder = canonical.to_string_lossy().into_owned();

    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            tracing::warn!(path = %path.display(), "claude folder-trust seed: read failed: {e}");
            return;
        }
    };

    match config_with_trusted_folder(&existing, &folder) {
        Ok(None) => {} // already trusted — nothing to write
        Ok(Some(updated)) => {
            if let Err(e) = write_atomic(&path, &updated) {
                tracing::warn!(path = %path.display(), "claude folder-trust seed: write failed: {e}");
            } else {
                tracing::info!(folder = %folder, "auto-trusted claude folder for headless launch");
            }
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), "claude folder-trust seed: parse failed: {e}");
        }
    }
}

/// Compute `.claude.json` contents that mark `folder` trusted, preserving every
/// existing key (top-level and per-project). `None` when already trusted.
///
/// A project entry Claude has never created carries more than the trust flag
/// (onboarding counters, history); we set only the keys that clear the dialog and
/// leave the rest to Claude, merging into any entry that already exists.
fn config_with_trusted_folder(existing: &str, folder: &str) -> anyhow::Result<Option<String>> {
    let (mut root, salvaged) = parse_or_salvage_root(existing)?;
    let projects = root
        .entry("projects".to_owned())
        .or_insert_with(|| json!({}));
    if !projects.is_object() {
        *projects = json!({});
    }
    let projects = projects.as_object_mut().expect("projects forced to object");
    let entry = projects
        .entry(folder.to_owned())
        .or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let entry = entry.as_object_mut().expect("entry forced to object");
    let already_trusted =
        entry.get("hasTrustDialogAccepted").and_then(Value::as_bool) == Some(true);
    // Skip the write only when the folder is *already* trusted AND the file was
    // clean. If we salvaged a corrupt config, write the repaired version back even
    // when the trust flag is already present — otherwise the corrupt file stays on
    // disk and the next agent parks on Claude's interactive "configuration error"
    // prompt (a headless agent can't answer it). Self-healing the config here is
    // what makes a concurrent-write corruption non-fatal.
    if already_trusted && !salvaged {
        return Ok(None);
    }
    entry.insert("hasTrustDialogAccepted".to_owned(), json!(true));
    // The dialog re-appears until onboarding is also marked seen; set both so a
    // brand-new entry doesn't fall back to the prompt. `or_insert` keeps any real
    // value Claude already recorded.
    entry
        .entry("hasCompletedProjectOnboarding".to_owned())
        .or_insert_with(|| json!(true));
    entry
        .entry("projectOnboardingSeenCount".to_owned())
        .or_insert_with(|| json!(1));

    let mut content = serde_json::to_string_pretty(&Value::Object(root))?;
    content.push('\n');
    Ok(Some(content))
}

/// Parse `existing` into the config's root object, **salvaging** a corrupt file
/// when a strict parse fails. Returns `(root, salvaged)` — `salvaged` is true when
/// the strict parse failed but a valid leading object was recovered, signalling the
/// caller to write the repaired config back.
///
/// Claude Code rewrites `~/.claude.json` in place on every startup; when two agents
/// start at once those writes race and can leave a complete object followed by
/// leftover bytes from a longer prior write (`}…` then a stray tail). The file is
/// then unparseable and Claude shows an interactive "configuration error" prompt
/// that wedges a headless agent. `serde_json`'s streaming reader parses the first
/// complete value and ignores trailing data, so we recover the valid prefix — the
/// newest good state — rather than discarding the whole config.
fn parse_or_salvage_root(existing: &str) -> anyhow::Result<(serde_json::Map<String, Value>, bool)> {
    if existing.trim().is_empty() {
        return Ok((serde_json::Map::new(), false));
    }
    // Strict parse first — the common, clean case.
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(existing) {
        return Ok((map, false));
    }
    // Strict parse failed. Recover the first complete JSON value (the valid prefix);
    // trailing garbage after it is dropped.
    let first = serde_json::Deserializer::from_str(existing)
        .into_iter::<Value>()
        .next();
    match first {
        Some(Ok(Value::Object(map))) => {
            tracing::warn!(
                "claude config was corrupt (trailing data after a valid object); \
                 salvaged the valid prefix and will rewrite it"
            );
            Ok((map, true))
        }
        // A leading non-object value, or nothing parseable at all: refuse rather
        // than clobber whatever is there with a guess.
        _ => anyhow::bail!(".claude.json is unparseable and could not be salvaged"),
    }
}

/// Write `content` to `path` via a temp file + rename so a concurrent Claude read
/// never sees a half-written config. Same-dir temp keeps the rename atomic.
fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.lazy-trust.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("claude")
    ));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_flag_in_empty_config() {
        let out = config_with_trusted_folder("", "/ws/a").unwrap().unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["projects"]["/ws/a"]["hasTrustDialogAccepted"],
            json!(true)
        );
        assert_eq!(
            v["projects"]["/ws/a"]["hasCompletedProjectOnboarding"],
            json!(true)
        );
    }

    #[test]
    fn preserves_other_projects_and_top_level_keys() {
        let existing = r#"{
  "numStartups": 7,
  "bypassPermissionsModeAccepted": true,
  "projects": {
    "/ws/a": { "hasTrustDialogAccepted": true, "history": [1, 2] }
  }
}"#;
        let out = config_with_trusted_folder(existing, "/ws/b")
            .unwrap()
            .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["numStartups"], json!(7));
        assert_eq!(v["bypassPermissionsModeAccepted"], json!(true));
        assert_eq!(v["projects"]["/ws/a"]["history"], json!([1, 2]));
        assert_eq!(
            v["projects"]["/ws/b"]["hasTrustDialogAccepted"],
            json!(true)
        );
    }

    #[test]
    fn merges_into_existing_untrusted_entry_without_clobbering() {
        let existing = r#"{
  "projects": {
    "/ws/a": { "hasTrustDialogAccepted": false, "projectOnboardingSeenCount": 0, "history": ["x"] }
  }
}"#;
        let out = config_with_trusted_folder(existing, "/ws/a")
            .unwrap()
            .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["projects"]["/ws/a"]["hasTrustDialogAccepted"],
            json!(true)
        );
        assert_eq!(v["projects"]["/ws/a"]["history"], json!(["x"]));
    }

    #[test]
    fn noop_when_already_trusted() {
        let existing = r#"{ "projects": { "/ws/a": { "hasTrustDialogAccepted": true } } }"#;
        assert!(
            config_with_trusted_folder(existing, "/ws/a")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_non_object_root() {
        assert!(config_with_trusted_folder("[1,2,3]", "/ws/a").is_err());
    }

    /// A config corrupted by a concurrent write (a complete object followed by
    /// leftover tail bytes) must be salvaged: the valid prefix is recovered, the
    /// folder trusted, top-level keys preserved, and a write forced so the repair
    /// lands on disk.
    #[test]
    fn salvages_config_with_trailing_garbage() {
        // Valid object, then a stray fragment from a racing in-place write.
        let corrupt = "{\n  \"bypassPermissionsModeAccepted\": true,\n  \
             \"projects\": { \"/ws/a\": { \"hasTrustDialogAccepted\": true } }\n}\nf9d5c\"\n}\n";
        // A strict parse must fail (proving this is the corrupt case)...
        assert!(serde_json::from_str::<Value>(corrupt).is_err());
        // ...yet we salvage it, force a rewrite (Some) even though /ws/a is already
        // trusted, and keep the critical top-level bypass key.
        let out = config_with_trusted_folder(corrupt, "/ws/a")
            .unwrap()
            .expect("a salvaged config is always rewritten");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["bypassPermissionsModeAccepted"], json!(true));
        assert_eq!(v["projects"]["/ws/a"]["hasTrustDialogAccepted"], json!(true));
    }

    /// Salvage also clears the trailing garbage for a *new* folder, merging the
    /// trust flag into the recovered prefix.
    #[test]
    fn salvages_and_trusts_new_folder() {
        let corrupt = "{ \"numStartups\": 3 }  oops-trailing";
        let out = config_with_trusted_folder(corrupt, "/ws/b")
            .unwrap()
            .expect("salvaged + new folder → rewrite");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["numStartups"], json!(3));
        assert_eq!(v["projects"]["/ws/b"]["hasTrustDialogAccepted"], json!(true));
    }

    /// Truly unrecoverable input (no leading valid object) errors rather than
    /// clobbering the file with a guess.
    #[test]
    fn unsalvageable_config_errors() {
        assert!(config_with_trusted_folder("}{ totally broken", "/ws/a").is_err());
    }

    #[test]
    fn write_atomic_creates_parent_and_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        // A nested, not-yet-existing config dir (mirrors a fresh CLAUDE_CONFIG_DIR).
        let path = tmp.path().join("nested").join(".claude.json");
        let seeded = config_with_trusted_folder("", "/ws/a").unwrap().unwrap();
        write_atomic(&path, &seeded).unwrap();

        // Round-trips to valid JSON with the folder trusted, and the temp file is
        // cleaned up by the rename (only the final config remains).
        let back = std::fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&back).unwrap();
        assert_eq!(
            v["projects"]["/ws/a"]["hasTrustDialogAccepted"],
            json!(true)
        );
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name() != "\u{2e}claude.json" && e.file_name() != ".claude.json")
            .collect();
        assert!(leftovers.is_empty(), "temp file should be renamed away");
    }
}
