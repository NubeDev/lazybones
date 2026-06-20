//! TAIL: drain hcom's raw event stream into the durable hcom log.
//!
//! The fourth tick phase (docs/hcom-logs-scope.md): after `claim_and_spawn`, pull
//! every hcom event newer than the ingestion cursor, key each to its `(run, task)`
//! by the launching tag, persist it, and advance the cursor. Best-effort and
//! self-contained like every tick phase — a failure is logged and never aborts
//! the pass. Restartable by construction: the cursor lives on the `Run`
//! (principle 3, no in-memory cross-tick state), so a restart resumes the drain.
//!
//! Because hcom's event id is **global**, not per-run, this does **one** drain per
//! tick from the minimum cursor across active runs and fans the result out by
//! resolved tag — cheaper than a pull per run, and the `(run, hcom_id)` upsert
//! makes the fan-out safe.

use std::collections::HashMap;

use lazybones_store::{Lifecycle, NewHcomLogEntry, Run, StoreHandle};

use crate::hcom::{Hcom, HcomEvent};

/// The supervisor tag prefix — a run-scoped agent launches as `sup:<run_id>`.
/// Shared contract with supervisor-scope.md (docs/hcom-logs-scope.md): the
/// supervisor spawn writes it, this tail parses it.
const SUP_TAG_PREFIX: &str = "sup:";

/// Drain hcom into the hcom log once. Best-effort: any failure is logged and the
/// pass continues.
pub async fn tail_hcom(store: &StoreHandle, hcom: &Hcom) {
    let runs = match store.list_runs().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("tail: list_runs failed, skipping: {e}");
            return;
        }
    };
    let active: Vec<Run> = runs
        .into_iter()
        .filter(|r| r.lifecycle == Lifecycle::Active)
        .collect();
    if active.is_empty() {
        return;
    }

    // 1. lo = min(hcom_log_cursor) over active runs (None treated as 0).
    let lo = active
        .iter()
        .map(|r| r.hcom_log_cursor.unwrap_or(0))
        .min()
        .unwrap_or(0);

    // 2. One non-blocking drain of everything newer.
    let events = match hcom.events_since(lo).await {
        Ok(ev) => ev,
        Err(e) => {
            tracing::warn!("tail: events_since({lo}) failed: {e}");
            return;
        }
    };
    if events.is_empty() {
        return;
    }

    // The agent name → tag map: hcom events carry `instance` (the agent name),
    // but we key on the launching `--tag`. `hcom list` exposes both.
    let name_to_tag = match agent_tags(hcom).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("tail: hcom list failed, cannot resolve tags: {e}");
            return;
        }
    };

    // Resolve each tag → (run-label, task) once; cache so we read each task at
    // most once a tick. `run_id` (the workflow FK) is tracked separately for the
    // cursor bump; the stored `run` label mirrors `event.run` so `GET
    // /runs/:id/hcom` parallels `GET /runs/:id`.
    let active_run_ids: std::collections::HashSet<&str> =
        active.iter().map(|r| r.id.as_str()).collect();
    let mut resolved: HashMap<String, Option<Resolved>> = HashMap::new();

    // Track the max hcom_id ingested per run_id, to advance cursors after writes.
    let mut max_by_run: HashMap<String, u64> = HashMap::new();

    for ev in &events {
        let Some(hcom_id) = ev.id_int() else {
            tracing::warn!(instance = %ev.instance, "tail: event with non-integer id, skipping");
            continue;
        };
        let Some(tag) = name_to_tag.get(&ev.instance) else {
            // An agent we didn't launch (or already reaped from `hcom list`): the
            // hcom log is scoped to lazybones-launched agents, so drop it.
            continue;
        };

        let home = match resolved.get(tag) {
            Some(cached) => cached.clone(),
            None => {
                let home = resolve_tag(store, tag).await;
                resolved.insert(tag.clone(), home.clone());
                home
            }
        };
        let Some(home) = home else {
            continue; // unknown tag → dropped
        };

        // Scoped to workflow runs: the ingestion cursor lives on a `Run`, so a
        // standalone task (no `run_id`) has no cursor to advance and is left for a
        // later phase rather than re-drained every tick (and possibly missed once
        // `lo` advances past it). Drop events that don't resolve to an active run.
        let Some(run_id) = home.run_id.as_deref() else {
            continue;
        };
        if !active_run_ids.contains(run_id) {
            continue;
        }

        if append_one(store, ev, hcom_id, tag, &home).await {
            let entry = max_by_run.entry(run_id.to_owned()).or_insert(0);
            *entry = (*entry).max(u64::try_from(hcom_id).unwrap_or(0));
        }
    }

    // 5. Advance each touched run's cursor AFTER the rows are written, so a crash
    //    between write and cursor-bump only re-ingests (harmless), never skips.
    for (run_id, max_id) in max_by_run {
        if let Err(e) = store.advance_hcom_cursor(&run_id, max_id).await {
            tracing::warn!(run = %run_id, "tail: advance_hcom_cursor failed: {e}");
        }
    }
}

/// Where an event lands: its `run` label (mirrors `event.run`), the `task` it
/// belongs to (None for run-scoped supervisors), and the workflow `run_id` whose
/// cursor advances (None for a standalone task).
#[derive(Debug, Clone)]
struct Resolved {
    run: String,
    task: Option<String>,
    run_id: Option<String>,
}

/// Resolve a launching tag to its home, per docs/hcom-logs-scope.md:
/// - a known task id → `task = id`, `run = task.run`, `run_id = task.run_id`;
/// - `sup:<run_id>` → run-scoped, `task = None`, `run`/`run_id` from the run;
/// - anything else → `None` (unknown → dropped).
async fn resolve_tag(store: &StoreHandle, tag: &str) -> Option<Resolved> {
    if let Some(run_id) = tag.strip_prefix(SUP_TAG_PREFIX) {
        // Supervisor: resolve the run for its label; the run id is its own label.
        return match store.get_run(run_id).await {
            Ok(Some(run)) => Some(Resolved {
                run: run.id.clone(),
                task: None,
                run_id: Some(run.id),
            }),
            _ => None,
        };
    }
    match store.get_task(tag).await {
        Ok(Some(task)) => Some(Resolved {
            run: task.run.clone(),
            task: Some(task.id.clone()),
            run_id: task.run_id.clone(),
        }),
        _ => None,
    }
}

/// Append one resolved event to the hcom log; returns whether the write
/// succeeded (the caller advances the cursor only for successful writes).
async fn append_one(
    store: &StoreHandle,
    ev: &HcomEvent,
    hcom_id: i64,
    tag: &str,
    home: &Resolved,
) -> bool {
    let entry = NewHcomLogEntry {
        run: home.run.clone(),
        task: home.task.clone(),
        agent: ev.instance.clone(),
        tag: Some(tag.to_owned()),
        hcom_id,
        kind: ev.kind.clone(),
        data: ev.data.clone(),
        at: ev.ts.clone(),
    };
    match store.append_hcom_log(entry).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(agent = %ev.instance, hcom_id, "tail: append_hcom_log failed: {e}");
            false
        }
    }
}

/// Build the `agent name → launching tag` map from `hcom list`. Agents with no
/// tag (not lazybones-launched) are omitted.
async fn agent_tags(hcom: &Hcom) -> anyhow::Result<HashMap<String, String>> {
    let live = hcom.list().await?;
    Ok(live
        .into_iter()
        .filter_map(|a| a.tag.map(|tag| (a.name, tag)))
        .collect())
}
