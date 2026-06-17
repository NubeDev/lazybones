//! Parse `workfile.yaml` into seed tasks, resolving each `spec:` path.
//!
//! `spec:` is either inline text or a path to a `tasks/<id>.md` file relative to
//! the workfile; this resolves the path to its contents so the seed carries the
//! full spec text (the DB stores text, never a path — SCOPE.md, Documents). The
//! resulting seeds are posted to `POST /workfile/sync`.

use std::path::Path;

use lazybones_store::SeedTask;
use serde::Deserialize;

/// A task as authored in `workfile.yaml`.
#[derive(Debug, Deserialize)]
struct WorkfileTask {
    id: String,
    title: String,
    spec: String,
    #[serde(default)]
    deps: Vec<String>,
    #[serde(default)]
    owns: Vec<String>,
    #[serde(default)]
    tool: Option<String>,
}

/// The `workfile.yaml` document.
#[derive(Debug, Deserialize)]
struct Workfile {
    #[serde(default)]
    tasks: Vec<WorkfileTask>,
}

/// Parse `workfile.yaml` at `path`, resolving spec paths relative to its dir.
///
/// # Errors
/// Returns an error if the workfile or a referenced spec file cannot be read or
/// parsed.
pub fn parse_workfile(path: &Path) -> anyhow::Result<Vec<SeedTask>> {
    let doc: Workfile = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));

    doc.tasks
        .into_iter()
        .map(|t| {
            let spec = resolve_spec(&t.spec, base)?;
            Ok(SeedTask {
                id: t.id,
                title: t.title,
                spec,
                deps: t.deps,
                owns: t.owns,
                tool: t.tool,
            })
        })
        .collect()
}

/// Resolve a `spec:` value: a `.md` path is read from disk; anything else is
/// treated as inline spec text.
fn resolve_spec(spec: &str, base: &Path) -> anyhow::Result<String> {
    let candidate = base.join(spec);
    if spec.ends_with(".md") && candidate.exists() {
        Ok(std::fs::read_to_string(candidate)?)
    } else {
        Ok(spec.to_owned())
    }
}
