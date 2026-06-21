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

use serde_json::{json, Value};

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
    let mut root: serde_json::Map<String, Value> = if existing.trim().is_empty() {
        serde_json::Map::new()
    } else {
        serde_json::from_str::<Value>(existing)?
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(".claude.json is not a JSON object"))?
    };
    let projects = root.entry("projects".to_owned()).or_insert_with(|| json!({}));
    if !projects.is_object() {
        *projects = json!({});
    }
    let projects = projects.as_object_mut().expect("projects forced to object");
    let entry = projects.entry(folder.to_owned()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let entry = entry.as_object_mut().expect("entry forced to object");
    if entry.get("hasTrustDialogAccepted").and_then(Value::as_bool) == Some(true) {
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

/// Write `content` to `path` via a temp file + rename so a concurrent Claude read
/// never sees a half-written config. Same-dir temp keeps the rename atomic.
fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.lazy-trust.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("claude")
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
        assert_eq!(v["projects"]["/ws/a"]["hasTrustDialogAccepted"], json!(true));
        assert_eq!(v["projects"]["/ws/a"]["hasCompletedProjectOnboarding"], json!(true));
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
        let out = config_with_trusted_folder(existing, "/ws/b").unwrap().unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["numStartups"], json!(7));
        assert_eq!(v["bypassPermissionsModeAccepted"], json!(true));
        assert_eq!(v["projects"]["/ws/a"]["history"], json!([1, 2]));
        assert_eq!(v["projects"]["/ws/b"]["hasTrustDialogAccepted"], json!(true));
    }

    #[test]
    fn merges_into_existing_untrusted_entry_without_clobbering() {
        let existing = r#"{
  "projects": {
    "/ws/a": { "hasTrustDialogAccepted": false, "projectOnboardingSeenCount": 0, "history": ["x"] }
  }
}"#;
        let out = config_with_trusted_folder(existing, "/ws/a").unwrap().unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["projects"]["/ws/a"]["hasTrustDialogAccepted"], json!(true));
        assert_eq!(v["projects"]["/ws/a"]["history"], json!(["x"]));
    }

    #[test]
    fn noop_when_already_trusted() {
        let existing = r#"{ "projects": { "/ws/a": { "hasTrustDialogAccepted": true } } }"#;
        assert!(config_with_trusted_folder(existing, "/ws/a").unwrap().is_none());
    }

    #[test]
    fn rejects_non_object_root() {
        assert!(config_with_trusted_folder("[1,2,3]", "/ws/a").is_err());
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
        assert_eq!(v["projects"]["/ws/a"]["hasTrustDialogAccepted"], json!(true));
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name() != "\u{2e}claude.json" && e.file_name() != ".claude.json")
            .collect();
        assert!(leftovers.is_empty(), "temp file should be renamed away");
    }
}
