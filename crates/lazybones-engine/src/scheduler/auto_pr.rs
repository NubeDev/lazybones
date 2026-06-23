//! Auto-open a GitHub PR when a workflow's last task finishes.
//!
//! Opt-in per workflow (`workspace.auto_pr`). On the tick that observes a run with
//! every task `done`, no PR yet recorded (`run.pr_url`), and auto-PR on, the engine:
//!
//! 1. Resolves the run's shared branch + a worktree to run `gh` in.
//! 2. Spawns the workflow's **configured agent** (same tool/model/effort the tasks
//!    used) in that worktree with a charter to read the diff and write a PR
//!    title + body to `.lazy/pr-summary.md`, then signal DONE on an hcom thread.
//! 3. Reads that file, `gh pr create`s the branch against base, and records the
//!    resulting url on the run (the idempotency guard — at most one PR per run).
//!
//! Best-effort and self-contained: any failure logs and leaves `pr_url` unset, so a
//! later tick retries. Never blocks the scheduler — it runs inside `reconcile`,
//! after claim/spawn, and a single spawn+await is bounded by `SUMMARY_AWAIT_SECS`.
//!
//! Why a spawned agent (not in-process LLM): the workflow already picked an agent
//! and its worktree is bootstrapped for headless runs, so reusing that path gives a
//! diff-grounded summary with zero extra wiring — the operator's agent choice wins.

use std::time::Duration;

use lazybones_store::{Lifecycle, Run, Status, StoreHandle, Task, WorktreeMode};

use crate::config::EngineConfig;
use crate::hcom::{AgentLaunch, Hcom};

use super::effective::{self, EffectiveGit};
use super::merge;

/// How long to wait for the summarizer agent to write its file + signal DONE.
/// Generous: it reads a diff and composes prose, but it is one bounded turn.
const SUMMARY_AWAIT_SECS: u64 = 900;

/// The file the summarizer writes (relative to the worktree). Line 1 is the PR
/// title; the rest is the body.
const SUMMARY_FILE: &str = ".lazy/pr-summary.md";

/// For each run that just completed and wants a PR, open one. Called once per tick
/// from `reconcile`. Iterates all runs; cheap when none are eligible (a status
/// scan), and the spawn only fires on the exact tick a run first goes all-done.
pub async fn open_prs_for_completed_runs(store: &StoreHandle, hcom: &Hcom, cfg: &EngineConfig) {
    let runs = match store.list_runs().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("auto_pr: list_runs failed: {e}");
            return;
        }
    };
    for run in runs {
        // Cheap gate first: opt-in on, not already opened.
        if run.workspace.auto_pr != Some(true) || run.pr_url.is_some() {
            continue;
        }
        // A stopped (paused) workflow drives no work — and must not keep an
        // auto_pr loop alive. This is the primary guard against issue #06's
        // forever-loop: a finished-then-stopped run (e.g. a branch already merged)
        // otherwise retries its impossible PR every tick, spawning an agent each
        // time. Stopped → leave it entirely alone until a human resumes it.
        if run.lifecycle == Lifecycle::Stopped {
            continue;
        }
        let tasks = match store.list_run_tasks(&run.id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(run = %run.id, "auto_pr: list_run_tasks failed: {e}");
                continue;
            }
        };
        if !all_done(&tasks) {
            continue;
        }
        if let Err(e) = open_pr(store, hcom, cfg, &run, &tasks).await {
            tracing::warn!(run = %run.id, "auto_pr: open PR failed (will retry next tick): {e}");
        }
    }
}

/// A run is PR-eligible only when it has tasks and every one is `done`.
fn all_done(tasks: &[Task]) -> bool {
    !tasks.is_empty() && tasks.iter().all(|t| t.status == Status::Done)
}

/// Resolve the branch + a worktree for `run`, spawn the summarizer, open the PR,
/// and record its url. Errors bubble to the caller, which logs and retries later.
async fn open_pr(
    store: &StoreHandle,
    hcom: &Hcom,
    cfg: &EngineConfig,
    run: &Run,
    tasks: &[Task],
) -> anyhow::Result<()> {
    // Use the first task's effective git so the branch/base/agent match the run's
    // resolved settings (in Shared mode every task shares them).
    let lead = tasks
        .first()
        .ok_or_else(|| anyhow::anyhow!("no tasks"))?;
    let eff = effective::resolve(lead, Some(run), cfg);

    let branch = run_branch(run, tasks, &eff)?;
    let worktree = worktree_dir(run, tasks, &eff, cfg)?;
    let wt = std::path::Path::new(&worktree);
    if !wt.is_dir() {
        anyhow::bail!("worktree {worktree} for PR summary is missing");
    }

    let gh = lazybones_gh::Gh::new();
    let summary_thread = format!("{}-autopr", run.id);

    // PRECHECK 1 — a PR already exists for this head branch (any state). The branch
    // was already PR'd (often already merged), so opening another is impossible and
    // `gh pr create` would fail "already exists" forever (issue #06). Record that
    // PR's url as this run's — it *is* the run's PR — which trips the idempotency
    // guard and ends the loop. Covers the merged-branch case (doc-writer / PR #2).
    if let Some(url) = existing_pr_url(&gh, wt, &branch).await {
        store.set_run_pr_url(&run.id, &url).await?;
        tracing::info!(run = %run.id, %url, "auto_pr: PR already exists for branch; recorded it (no spawn)");
        return Ok(());
    }

    // PRECHECK 2 — no commits between base and head. The branch is empty or fully
    // merged, so there is nothing to PR; `gh pr create` would fail "No commits
    // between …" every tick. Do NOT spawn a summarizer for an impossible PR. This
    // is a cheap status check (one `git rev-list --count`); it re-runs each tick
    // but launches no agent, so it cannot leak processes the way the old path did.
    let has_commits = merge::branch_has_commits(wt, &eff.base_branch, &branch)
        .await
        .unwrap_or(true);
    if !has_commits {
        tracing::info!(
            run = %run.id, %branch,
            "auto_pr: no commits between base and head; nothing to PR (no spawn)"
        );
        return Ok(());
    }

    // REAP before respawn: kill any summarizer agent left over from a prior attempt
    // for this run before launching a fresh one, so attempts can never accumulate
    // into a swarm (issue #06). Best-effort — no agents to kill is not an error.
    if let Err(e) = hcom.kill_tag(&summary_thread).await {
        tracing::debug!(run = %run.id, "auto_pr: reap of prior summarizer (best-effort): {e}");
    }

    // 1. Spawn the configured agent to write the summary file, then await DONE.
    spawn_summarizer(hcom, cfg, &eff, run, tasks, &worktree, &branch, &summary_thread).await?;
    // Whatever the await's outcome, make sure the summarizer doesn't linger as a
    // live agent past this attempt (timeout/BLOCKED would otherwise leave it idle).
    let awaited = await_summary(hcom, &summary_thread).await;
    let _ = hcom.kill_tag(&summary_thread).await;
    awaited?;

    // 2. Read the summary the agent wrote (title = first line, body = the rest).
    let (title, body) = read_summary(wt, run)?;

    // 3. Open the PR against base and record its url (idempotency guard).
    let url = match gh
        .pr_create(wt, &title, &body, &branch, &eff.base_branch, false)
        .await
    {
        Ok(url) => url.trim().to_owned(),
        Err(e) => {
            // A late race: a PR appeared between the precheck and now (or `gh`
            // reports it already exists). Recover the existing url rather than
            // treating it as a retryable failure — otherwise we'd loop forever.
            if let Some(url) = existing_pr_url(&gh, wt, &branch).await {
                store.set_run_pr_url(&run.id, &url).await?;
                tracing::info!(run = %run.id, %url, "auto_pr: PR already existed at create; recorded it");
                return Ok(());
            }
            return Err(e.into());
        }
    };
    store.set_run_pr_url(&run.id, &url).await?;
    tracing::info!(run = %run.id, %url, "auto_pr: opened PR");
    Ok(())
}

/// The url of an existing PR whose head is `branch` (any state), or `None`.
///
/// Used as the auto_pr idempotency/terminal check: if the branch was already
/// PR'd (open, closed, or merged), there is nothing to open and the existing PR
/// *is* the run's PR. Best-effort — any `gh` error yields `None` (treat as "no
/// known PR" and let the normal flow proceed/retry).
async fn existing_pr_url(gh: &lazybones_gh::Gh, wt: &std::path::Path, branch: &str) -> Option<String> {
    let prs = gh.prs(wt, lazybones_gh::PrState::All).await.ok()?;
    pick_pr_url(&prs, branch)
}

/// From `prs`, the url of the best PR whose head is `branch`: an open one if any,
/// else any (closed/merged). `None` when none match. Pure so it is unit-testable
/// without a live `gh`.
fn pick_pr_url(prs: &[lazybones_gh::PullRequest], branch: &str) -> Option<String> {
    let matching: Vec<&lazybones_gh::PullRequest> =
        prs.iter().filter(|p| p.head_ref == branch).collect();
    matching
        .iter()
        .find(|p| p.state.eq_ignore_ascii_case("open"))
        .or_else(|| matching.first())
        .map(|p| p.url.clone())
}

/// The run's branch: prefer a task's recorded branch (set on claim); else derive
/// the Shared name (`<prefix><run_id>`). Errors if neither is resolvable.
fn run_branch(run: &Run, tasks: &[Task], eff: &EffectiveGit) -> anyhow::Result<String> {
    if let Some(b) = tasks.iter().find_map(|t| t.branch.clone()) {
        return Ok(b);
    }
    if eff.worktree_mode == WorktreeMode::Shared {
        return Ok(format!("{}{}", eff.branch_prefix, run.id));
    }
    anyhow::bail!("no branch recorded on any task and run is not Shared")
}

/// A worktree to run `gh`/the summarizer in: prefer a task's recorded worktree;
/// else the Shared tree path. The repo root is a last resort (it's always a valid
/// `gh` cwd even if the worktree was reaped).
fn worktree_dir(
    run: &Run,
    tasks: &[Task],
    eff: &EffectiveGit,
    cfg: &EngineConfig,
) -> anyhow::Result<String> {
    if let Some(w) = tasks.iter().find_map(|t| t.worktree.clone())
        && std::path::Path::new(&w).is_dir()
    {
        return Ok(w);
    }
    if eff.worktree_mode == WorktreeMode::Shared {
        let p = eff.repo.join(&cfg.worktree_root).join(&run.id);
        if p.is_dir() {
            return Ok(p.to_string_lossy().into_owned());
        }
    }
    // Fall back to the repo root — the branch still exists there to PR from.
    Ok(eff.repo.to_string_lossy().into_owned())
}

/// Spawn the workflow's agent with a charter to write `.lazy/pr-summary.md` and
/// signal DONE on `thread`. Reuses the per-tool gate-bypass flags + agent triple.
#[allow(clippy::too_many_arguments)]
async fn spawn_summarizer(
    hcom: &Hcom,
    cfg: &EngineConfig,
    eff: &EffectiveGit,
    run: &Run,
    tasks: &[Task],
    worktree: &str,
    branch: &str,
    thread: &str,
) -> anyhow::Result<()> {
    let prompt = summary_prompt(run, tasks, &eff.base_branch, branch, thread);
    let perm_flags = cfg
        .permission_flags
        .get(&eff.tool)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    hcom.spawn(
        &eff.tool,
        thread,
        std::path::Path::new(worktree),
        &prompt,
        AgentLaunch {
            model: eff.model.as_deref(),
            effort: eff.effort.as_deref(),
            permission_flags: perm_flags,
        },
    )
    .await?;
    Ok(())
}

/// The summarizer charter: read the diff + task list, write title+body to the
/// summary file, signal DONE. No commit/push — this is a read + one file write.
fn summary_prompt(run: &Run, tasks: &[Task], base: &str, branch: &str, thread: &str) -> String {
    let task_lines = tasks
        .iter()
        .map(|t| format!("- {}: {}", t.id, t.title))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "You are summarizing a completed workflow to open a pull request.\n\
         \n\
         The workflow `{run_id}` ({run_title}) ran these tasks, all now done:\n\
         {task_lines}\n\
         \n\
         Their combined work is on branch `{branch}`, to be merged into `{base}`.\n\
         \n\
         Do this, then stop:\n\
         1. Read the change: `git diff {base}...{branch}` and `git log {base}..{branch}`.\n\
         2. Write a pull-request summary to the file `{file}` (create the `.lazy`\n\
         directory if needed). The VERY FIRST LINE is the PR title (one concise line,\n\
         no leading `#`). Every line after it is the PR body in Markdown: a short\n\
         overview, then a bullet list of the notable changes, then a `## Tasks`\n\
         section listing each task. Keep it factual and grounded in the actual diff.\n\
         3. Do NOT commit, push, or open the PR yourself — only write the file.\n\
         4. Signal completion exactly once on the hcom thread `{thread}`:\n\
         `hcom send @all --thread {thread} -- DONE`\n\
         (or `... -- BLOCKED: <reason>` if you cannot).\n\
         \n\
         Do not touch files outside this worktree other than `{file}`. Never write\n\
         memory notes or edit anything under any `.claude/` directory. Never stop to\n\
         ask for permission.\n",
        run_id = run.id,
        run_title = run.title,
        file = SUMMARY_FILE,
    )
}

/// Block until the summarizer signals DONE/BLOCKED on its thread, or times out.
async fn await_summary(hcom: &Hcom, thread: &str) -> anyhow::Result<()> {
    let sql = format!(
        "type = 'message' AND json_extract(data, '$.thread') = '{thread}' \
         AND (json_extract(data, '$.text') LIKE '%DONE%' \
         OR json_extract(data, '$.text') LIKE '%BLOCKED%')"
    );
    let events = hcom.wait(&sql, Duration::from_secs(SUMMARY_AWAIT_SECS)).await?;
    let Some(ev) = events.first() else {
        anyhow::bail!("summarizer timed out with no DONE signal");
    };
    let text = ev
        .data
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if text.contains("BLOCKED") {
        anyhow::bail!("summarizer reported BLOCKED: {text}");
    }
    Ok(())
}

/// Read the agent-written summary file → `(title, body)`. The first non-empty line
/// is the title; the remainder is the body. Falls back to a deterministic summary
/// if the file is missing or empty (so a PR still opens with task context).
fn read_summary(worktree: &std::path::Path, run: &Run) -> anyhow::Result<(String, String)> {
    let path = worktree.join(SUMMARY_FILE);
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        // No file (agent skipped/failed the write) — still open a PR titled by the
        // workflow so the branch isn't stranded; body notes the missing summary.
        return Ok((
            format!("{} ({})", run.title, run.id),
            "_Auto-generated PR: the summary agent produced no summary file._".to_owned(),
        ));
    }
    let mut lines = raw.lines();
    let title = lines.next().unwrap_or(&run.title).trim().to_owned();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    Ok((title, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazybones_store::Workspace;

    fn task(id: &str, status: Status) -> Task {
        let mut t = Task::seed(id, "wf", id, "s", vec![], vec![], None);
        t.status = status;
        t
    }

    fn run_shared() -> Run {
        let ws = Workspace {
            repo: "/repo".into(),
            base_branch: Some("main".into()),
            branch_prefix: Some("lazy/".into()),
            worktree_mode: WorktreeMode::Shared,
            tool: None,
            model: None,
            effort: None,
            gate: None,
            merge: None,
            auto_pr: Some(true),
        };
        Run::new("simple-demo", "Simple Demo", ws, "2026-01-01T00:00:00Z")
    }

    #[test]
    fn all_done_requires_tasks_and_every_one_done() {
        assert!(!all_done(&[]), "no tasks → not eligible");
        assert!(!all_done(&[task("a", Status::Done), task("b", Status::Running)]));
        assert!(all_done(&[task("a", Status::Done), task("b", Status::Done)]));
    }

    #[test]
    fn run_branch_derives_shared_name_when_no_task_branch() {
        let run = run_shared();
        let eff = effective::resolve(&task("a", Status::Done), Some(&run), &cfg());
        // No task carries a branch → derive `<prefix><run_id>`.
        let b = run_branch(&run, &[task("a", Status::Done)], &eff).unwrap();
        assert_eq!(b, "lazy/simple-demo");
    }

    #[test]
    fn run_branch_prefers_a_recorded_task_branch() {
        let run = run_shared();
        let eff = effective::resolve(&task("a", Status::Done), Some(&run), &cfg());
        let mut t = task("a", Status::Done);
        t.branch = Some("lazy/simple-demo".into());
        assert_eq!(run_branch(&run, &[t], &eff).unwrap(), "lazy/simple-demo");
    }

    #[test]
    fn summary_prompt_grounds_the_agent_in_diff_and_tasks() {
        let run = run_shared();
        let tasks = [task("scaffold", Status::Done), task("review", Status::Done)];
        let p = summary_prompt(&run, &tasks, "main", "lazy/simple-demo", "simple-demo-autopr");
        // Tells the agent to diff, write the file, NOT commit, and signal DONE.
        assert!(p.contains("git diff main...lazy/simple-demo"));
        assert!(p.contains(SUMMARY_FILE));
        assert!(p.contains("Do NOT commit"));
        assert!(p.contains("--thread simple-demo-autopr -- DONE"));
        // Lists each task so the body can enumerate them.
        assert!(p.contains("- scaffold:"));
        assert!(p.contains("- review:"));
    }

    #[test]
    fn read_summary_splits_title_from_body() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".lazy")).unwrap();
        std::fs::write(
            dir.path().join(SUMMARY_FILE),
            "Add usage docs\n\nOverview here.\n\n## Tasks\n- scaffold\n",
        )
        .unwrap();
        let (title, body) = read_summary(dir.path(), &run_shared()).unwrap();
        assert_eq!(title, "Add usage docs");
        assert!(body.starts_with("Overview here."));
        assert!(body.contains("## Tasks"));
    }

    #[test]
    fn read_summary_falls_back_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let (title, body) = read_summary(dir.path(), &run_shared()).unwrap();
        // A PR still opens (branch isn't stranded), titled by the workflow.
        assert!(title.contains("Simple Demo"));
        assert!(body.contains("no summary file"));
    }

    fn cfg() -> EngineConfig {
        EngineConfig {
            target_repo: "/repo".into(),
            base_branch: "main".into(),
            remote: "origin".into(),
            gate: vec![],
            concurrency: 3,
            worktrees: true,
            worktree_root: ".lazy/wt".into(),
            branch_prefix: "lazy/".into(),
            merge: crate::config::MergeMode::Pr,
            agent_tool: "claude".into(),
            agent_model: None,
            agent_effort: None,
            permission_flags: std::collections::HashMap::new(),
            auto_trust_agent_folder: true,
            stale_after_secs: 300,
            tick_secs: 2,
            issue_sync_every_n_ticks: 0,
        }
    }

    fn pr_json(branch: &str, state: &str, url: &str) -> lazybones_gh::PullRequest {
        serde_json::from_str(&format!(
            r#"{{"number":1,"title":"t","state":"{state}","url":"{url}","headRefName":"{branch}","baseRefName":"main"}}"#
        ))
        .unwrap()
    }

    #[test]
    fn pick_pr_url_matches_head_branch_and_prefers_open() {
        let prs = vec![
            pr_json("other", "OPEN", "u-other"),
            pr_json("lazy/run-1", "CLOSED", "u-closed"),
            pr_json("lazy/run-1", "OPEN", "u-open"),
        ];
        // The open PR on the matching head wins over the closed one.
        assert_eq!(
            pick_pr_url(&prs, "lazy/run-1").as_deref(),
            Some("u-open")
        );
    }

    #[test]
    fn pick_pr_url_falls_back_to_a_merged_pr() {
        // The merged-branch case (issue #06: doc-writer's branch merged via its PR).
        // No open PR, but a merged one exists for the head → record it, terminal.
        let prs = vec![pr_json("lazy/doc-writer", "MERGED", "u-merged")];
        assert_eq!(
            pick_pr_url(&prs, "lazy/doc-writer").as_deref(),
            Some("u-merged")
        );
    }

    #[test]
    fn pick_pr_url_none_when_no_head_matches() {
        let prs = vec![pr_json("other", "OPEN", "u")];
        assert_eq!(pick_pr_url(&prs, "lazy/run-1"), None);
        assert_eq!(pick_pr_url(&[], "lazy/run-1"), None);
    }
}
