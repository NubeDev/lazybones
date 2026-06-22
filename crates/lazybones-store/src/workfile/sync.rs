//! Idempotent import of seed tasks into the store.
//!
//! Upserts every seed (lifecycle preserved on re-import) then wires the
//! `depends_on` edges once all task endpoints exist. Shared by the REST sync
//! route and the CLI boot import so there is one import path.

use std::collections::HashSet;

use crate::error::Result;
use crate::handle::StoreHandle;
use crate::task::Task;

use super::seed::SeedTask;

/// Upsert `seeds` into `run` and relate their dependency edges.
///
/// Returns the number of tasks synced.
///
/// # Errors
/// Returns a [`StoreError`](crate::StoreError) if any upsert or relate fails.
pub async fn sync_seeds(store: &StoreHandle, run: &str, seeds: &[SeedTask]) -> Result<usize> {
    // A reuse source counts as a real dependency only if it's a task we can
    // actually order against — one defined in this same batch. An unknown source
    // (typo, or a task in another workflow) is left to the claim-time
    // `resolve_reuse` guard instead of wedging the task `pending` on a ghost dep.
    let known: HashSet<&str> = seeds.iter().map(|s| s.id.as_str()).collect();
    let effective = |s: &SeedTask| deps_with_reuse(&s.deps, reuse_dep(s, &known));

    for seed in seeds {
        let mut task = Task::seed(
            &seed.id,
            run,
            &seed.title,
            &seed.spec,
            effective(seed),
            seed.owns.clone(),
            seed.tool.clone(),
        );
        task.reuse_from = seed.reuse_from.clone();
        store.upsert_task(&task).await?;
    }
    for seed in seeds {
        for dep in &effective(seed) {
            store.relate_dep(&seed.id, dep).await?;
        }
    }
    Ok(seeds.len())
}

/// The `reuse_from` source to treat as a dependency: `Some` only when the source
/// is a known task in `known`, else `None` (left to the claim-time guard).
fn reuse_dep<'a>(seed: &'a SeedTask, known: &HashSet<&str>) -> Option<&'a str> {
    seed.reuse_from
        .as_deref()
        .filter(|src| known.contains(src))
}

/// The effective dependency list for a task: its authored `deps` plus a known
/// `reuse_from` source, if any, when that source isn't already a dep.
///
/// `reuse_from` is a data dependency — the source's worktree must exist before a
/// `reuse` task can continue it — so a *known* source belongs in the dependency
/// graph. Folding it into `deps` keeps one source of truth: readiness, the plan
/// graph, and the stored `deps` all agree, with no parallel reuse-edge to sync.
/// Unknown sources are excluded by the caller so they don't wedge the task on a
/// dependency this run can never resolve.
#[must_use]
pub fn deps_with_reuse(deps: &[String], reuse_from: Option<&str>) -> Vec<String> {
    let mut out = deps.to_vec();
    if let Some(source) = reuse_from
        && !out.iter().any(|d| d == source)
    {
        out.push(source.to_owned());
    }
    out
}
