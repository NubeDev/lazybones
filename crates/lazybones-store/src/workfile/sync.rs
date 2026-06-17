//! Idempotent import of seed tasks into the store.
//!
//! Upserts every seed (lifecycle preserved on re-import) then wires the
//! `depends_on` edges once all task endpoints exist. Shared by the REST sync
//! route and the CLI boot import so there is one import path.

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
    for seed in seeds {
        let task = Task::seed(
            &seed.id,
            run,
            &seed.title,
            &seed.spec,
            seed.deps.clone(),
            seed.owns.clone(),
            seed.tool.clone(),
        );
        store.upsert_task(&task).await?;
    }
    for seed in seeds {
        for dep in &seed.deps {
            store.relate_dep(&seed.id, dep).await?;
        }
    }
    Ok(seeds.len())
}
