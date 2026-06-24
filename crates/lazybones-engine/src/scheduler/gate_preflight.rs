//! Validate a run's gate against its target repo *before* the first task spawns,
//! and auto-fix a gate that can't structurally run there.
//!
//! ## Why this exists
//!
//! The baked-in default gate (`cargo test --workspace`,
//! `cargo clippy --workspace --all-targets -- -D warnings`) fits a single Cargo
//! **workspace** — which is exactly what *lazybones-the-repo* is. But lazybones
//! also drives *other* repos, and a repo whose crates are independent (no root
//! `[workspace]` table — e.g. `rbx-server` + `app-server` + `pii-redactor` side by
//! side) makes `--workspace` fail with *"could not find `Cargo.toml` … or any
//! parent directory"* / *"… is not a workspace"* before a single test runs.
//!
//! That failure is **not** a code failure a retry or a code fix can clear: the
//! command itself is wrong for the repo, so every retry re-hits the same wall and
//! the task wedges. [`super::follow_up`] used to (correctly) treat a red gate as
//! ordinary work — but a *structurally inapplicable* gate is a different animal.
//!
//! ## What it does
//!
//! [`check`] is a pure, filesystem-only preflight (no network, no process spawn —
//! cheap enough to run before claim). When the gate uses `cargo … --workspace` but
//! the repo is not a workspace, it discovers the repo's real member crates and
//! rewrites each `--workspace` command into one `--manifest-path <crate>/Cargo.toml`
//! command per crate. With no crates to target, it reports the gate unfixable so
//! the caller can surface a `gate-config` follow-up instead of looping.
//!
//! Non-cargo gates, and cargo gates that don't use `--workspace`, pass through
//! untouched — this only knows how to repair the one mismatch it understands.

use std::path::Path;

/// The outcome of preflighting a gate against a repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preflight {
    /// The gate runs against this repo as configured — nothing to do.
    Ok,
    /// The gate was rewritten to fit the repo. `gate` is the replacement command
    /// list; `note` explains the rewrite (for the log + the operator).
    Fixed { gate: Vec<String>, note: String },
    /// The gate can't run here and can't be auto-fixed. `reason` is phrased so
    /// [`super::follow_up::classify`] recognises it as a `gate-config` wall.
    Unfixable { reason: String },
}

/// How deep below the repo root to look for member crates. Most repos that aren't
/// a single workspace still keep their crates one (`rbx-server/`) or two
/// (`services/api/`) levels down; going deeper risks pulling in vendored or
/// example crates, so we stop here.
const CRATE_SCAN_DEPTH: usize = 2;

/// Preflight `gate` against `repo`. Pure except for reads under `repo`.
///
/// Only the `cargo … --workspace`-against-a-non-workspace mismatch is repaired;
/// every other gate returns [`Preflight::Ok`] unchanged.
#[must_use]
pub fn check(repo: &Path, gate: &[String]) -> Preflight {
    // Nothing configured, or no command leans on `--workspace`: leave it be.
    if !gate.iter().any(|c| uses_cargo_workspace(c)) {
        return Preflight::Ok;
    }
    // A real workspace root makes `--workspace` valid — the common, happy case
    // (lazybones-the-repo, and any other single-workspace target).
    if is_cargo_workspace(repo) {
        return Preflight::Ok;
    }

    // `--workspace` but no workspace root: the gate can't run as written. Find the
    // crates that actually live here so we can target them individually.
    let crates = member_crates(repo);
    if crates.is_empty() {
        return Preflight::Unfixable {
            reason: format!(
                "gate uses `cargo … --workspace` but {} is not a cargo workspace \
                 (no root `Cargo.toml` with a `[workspace]` table) and no member \
                 crates were found to target instead",
                repo.display()
            ),
        };
    }

    // Rewrite each `--workspace` cargo command into one per crate; pass everything
    // else through unchanged and in place.
    let mut fixed = Vec::new();
    for cmd in gate {
        if uses_cargo_workspace(cmd) {
            for krate in &crates {
                fixed.push(rewrite_for_crate(cmd, krate));
            }
        } else {
            fixed.push(cmd.clone());
        }
    }
    let note = format!(
        "{} is not a cargo workspace; rewrote the `--workspace` gate to run \
         per-crate against {} discovered crate(s): {}",
        repo.display(),
        crates.len(),
        crates.join(", ")
    );
    Preflight::Fixed { gate: fixed, note }
}

/// Does `cmd` invoke cargo with `--workspace`? Tokenised on whitespace so a flag
/// embedded in a path or string literal doesn't false-match.
fn uses_cargo_workspace(cmd: &str) -> bool {
    let toks: Vec<&str> = cmd.split_whitespace().collect();
    toks.contains(&"cargo") && toks.contains(&"--workspace")
}

/// Is `repo` the root of a cargo workspace? True only when a root `Cargo.toml`
/// exists *and* declares a `[workspace]` table — a plain package manifest at the
/// root is not a workspace and still breaks `--workspace`.
fn is_cargo_workspace(repo: &Path) -> bool {
    match std::fs::read_to_string(repo.join("Cargo.toml")) {
        Ok(text) => declares_workspace(&text),
        Err(_) => false,
    }
}

/// Crude-but-sufficient check that a manifest declares a `[workspace]` table.
/// Matches a `[workspace]` or `[workspace.members]`-style header at the start of a
/// line, ignoring leading whitespace; tolerant of inline comments after it.
fn declares_workspace(manifest: &str) -> bool {
    manifest.lines().any(|line| {
        let line = line.trim_start();
        line == "[workspace]" || line.starts_with("[workspace]") || line.starts_with("[workspace.")
    })
}

/// Discover member crates under `repo`: directories containing a `Cargo.toml`,
/// searched to [`CRATE_SCAN_DEPTH`]. Returns repo-relative, slash-joined paths
/// (e.g. `rbx-server`, `services/api`) sorted for a stable, deterministic gate.
/// Skips hidden dirs and the usual non-source noise (`target`, `.lazy`, …).
fn member_crates(repo: &Path) -> Vec<String> {
    let mut found = Vec::new();
    collect_crates(repo, repo, 0, &mut found);
    found.sort();
    found
}

/// Recursive worker for [`member_crates`]. `root` is the repo root (for relative
/// paths); `dir` is the directory being scanned at `depth`.
fn collect_crates(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Hidden dirs (`.lazy`, `.git`), build output, and dependency caches never
        // hold a member crate we want to gate.
        if name.starts_with('.') || matches!(name.as_ref(), "target" | "node_modules") {
            continue;
        }
        if path.join("Cargo.toml").is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
            // A crate dir's own children are its modules, not sibling crates — don't
            // descend into it.
            continue;
        }
        if depth + 1 < CRATE_SCAN_DEPTH {
            collect_crates(root, &path, depth + 1, out);
        }
    }
}

/// Rewrite a single `cargo … --workspace …` command to target one crate via
/// `--manifest-path <crate>/Cargo.toml`, dropping `--workspace`. Token order is
/// preserved; the `--manifest-path` pair is inserted right after the subcommand
/// (e.g. `cargo test --manifest-path rbx-server/Cargo.toml --all-targets`).
fn rewrite_for_crate(cmd: &str, krate: &str) -> String {
    let toks: Vec<&str> = cmd.split_whitespace().collect();
    let mut out: Vec<String> = Vec::with_capacity(toks.len() + 1);
    let mut inserted = false;
    for (i, tok) in toks.iter().enumerate() {
        if *tok == "--workspace" {
            continue; // dropped: a single crate is not a workspace
        }
        out.push((*tok).to_owned());
        // Insert the manifest path immediately after the cargo subcommand (the
        // token right after `cargo`), so it binds before any `--` separator.
        if !inserted && i >= 1 && toks[i - 1] == "cargo" {
            out.push("--manifest-path".to_owned());
            out.push(format!("{krate}/Cargo.toml"));
            inserted = true;
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A repo with a root workspace manifest — the gate is valid, untouched.
    #[test]
    fn workspace_repo_passes_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"a\"]\n",
        )
        .unwrap();
        let gate = vec!["cargo test --workspace".to_owned()];
        assert_eq!(check(dir.path(), &gate), Preflight::Ok);
    }

    /// The rubix-cube shape: no root manifest, independent crates in subdirs. The
    /// `--workspace` gate is rewritten to one command per crate.
    #[test]
    fn non_workspace_repo_rewrites_per_crate() {
        let dir = tempfile::tempdir().unwrap();
        for krate in ["rbx-server", "app-server"] {
            let cd = dir.path().join(krate);
            fs::create_dir_all(&cd).unwrap();
            fs::write(cd.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
        }
        let gate = vec![
            "cargo test --workspace".to_owned(),
            "cargo clippy --workspace --all-targets -- -D warnings".to_owned(),
        ];
        let Preflight::Fixed { gate: fixed, .. } = check(dir.path(), &gate) else {
            panic!("expected a rewrite");
        };
        assert_eq!(
            fixed,
            vec![
                "cargo test --manifest-path app-server/Cargo.toml".to_owned(),
                "cargo test --manifest-path rbx-server/Cargo.toml".to_owned(),
                "cargo clippy --manifest-path app-server/Cargo.toml --all-targets -- -D warnings"
                    .to_owned(),
                "cargo clippy --manifest-path rbx-server/Cargo.toml --all-targets -- -D warnings"
                    .to_owned(),
            ]
        );
    }

    /// A root manifest that's a plain package (no `[workspace]`) is still not a
    /// workspace — discover the package itself as the lone crate and target it.
    #[test]
    fn root_package_without_workspace_is_targeted() {
        let dir = tempfile::tempdir().unwrap();
        // root is a package, plus a nested crate
        let cd = dir.path().join("inner");
        fs::create_dir_all(&cd).unwrap();
        fs::write(cd.join("Cargo.toml"), "[package]\nname=\"i\"\n").unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"r\"\n").unwrap();
        let gate = vec!["cargo test --workspace".to_owned()];
        let Preflight::Fixed { gate: fixed, .. } = check(dir.path(), &gate) else {
            panic!("expected a rewrite");
        };
        assert_eq!(fixed, vec!["cargo test --manifest-path inner/Cargo.toml".to_owned()]);
    }

    /// No workspace and no crates anywhere → unfixable, with a reason the
    /// follow-up classifier recognises.
    #[test]
    fn no_crates_is_unfixable() {
        let dir = tempfile::tempdir().unwrap();
        let gate = vec!["cargo test --workspace".to_owned()];
        let Preflight::Unfixable { reason } = check(dir.path(), &gate) else {
            panic!("expected unfixable");
        };
        assert!(reason.contains("not a cargo workspace"));
    }

    /// A gate that never uses `--workspace` is out of scope — untouched even with
    /// no manifest at all (could be a Node repo, a per-crate cargo gate, …).
    #[test]
    fn non_workspace_gate_passes_through() {
        let dir = tempfile::tempdir().unwrap();
        let gate = vec!["npm test".to_owned(), "cargo test -p foo".to_owned()];
        assert_eq!(check(dir.path(), &gate), Preflight::Ok);
    }

    /// `target/` and hidden dirs must not be mistaken for member crates.
    #[test]
    fn scan_skips_target_and_hidden() {
        let dir = tempfile::tempdir().unwrap();
        for noise in ["target", ".lazy"] {
            let cd = dir.path().join(noise);
            fs::create_dir_all(&cd).unwrap();
            fs::write(cd.join("Cargo.toml"), "[package]\nname=\"n\"\n").unwrap();
        }
        let real = dir.path().join("rbx-server");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("Cargo.toml"), "[package]\nname=\"r\"\n").unwrap();
        let gate = vec!["cargo test --workspace".to_owned()];
        let Preflight::Fixed { gate: fixed, .. } = check(dir.path(), &gate) else {
            panic!("expected a rewrite");
        };
        assert_eq!(fixed, vec!["cargo test --manifest-path rbx-server/Cargo.toml".to_owned()]);
    }
}
