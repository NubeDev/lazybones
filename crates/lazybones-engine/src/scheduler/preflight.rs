//! Workspace preflight — the "this will not work, don't run it" lock.
//!
//! Run at `POST /workflows/:id/start`, *before* a workflow goes live. It catches
//! the misconfigurations that silently corrupt a run — the ones that bit us once
//! and are invisible until tasks are already committing/merging:
//!
//! 1. **The repo isn't a git work tree.** Provisioning would fail per-task with an
//!    opaque error; better to say so up front.
//! 2. **`.lazy/` isn't gitignored.** The daemon writes its db *and every worktree*
//!    under `<repo>/.lazy/`. If that path isn't ignored, a task's `git add -A`
//!    stages the daemon's own state (including nested worktrees) into the branch —
//!    which then conflicts/corrupts when the branch merges into `base_branch`. This
//!    is the "worktree-bleed" that interleaved daemon state with real commits.
//!
//! Each returns a blocking, actionable message. An empty result means "safe to
//! start." Checks are best-effort: a git invocation that errors outright is
//! reported as its own problem rather than silently passing.
//!
//! Deliberately *not* a check: a dirty `base_branch` working tree. Task worktrees
//! are isolated checkouts and a landing merge moves `base_branch`'s HEAD without
//! touching its tree, so a dirty main checkout is normal once a workflow is running
//! — blocking on it false-positives every multi-task/concurrent run.

use std::path::Path;

use super::git::git;

/// Validate a workflow's workspace against the walls that corrupt a run. Returns
/// one human-readable, actionable message per blocking problem; an empty vec means
/// the workspace is safe to start.
///
/// `repo` is the workspace repo path; `base_branch` is the branch tasks fork from
/// and land into. `worktree_root` is the daemon's per-repo state dir relative to
/// the repo (e.g. `.lazy/wt`); its top component (`.lazy`) is what must be ignored.
pub async fn workspace_preflight(
    repo: &Path,
    base_branch: &str,
    worktree_root: &str,
) -> Vec<String> {
    let mut problems = Vec::new();

    // 1. The repo must be a git work tree, or nothing else is meaningful.
    if !repo.is_dir() {
        problems.push(format!(
            "repo path {} does not exist — set workspace.repo to an absolute path to a git checkout",
            repo.display()
        ));
        return problems;
    }
    match git(repo, &["rev-parse", "--is-inside-work-tree"]).await {
        Ok(o) if o.ok && o.stdout.trim() == "true" => {}
        _ => {
            problems.push(format!(
                "repo {} is not a git work tree — the daemon provisions task worktrees with `git worktree add`, which needs a real repo",
                repo.display()
            ));
            return problems;
        }
    }

    // 2. The daemon's worktree root must be gitignored, or tasks commit it and
    //    break the merge. Check the actual worktree path (`.lazy/wt`): a directory
    //    rule like `.lazy/` matches everything under it, so this passes whether the
    //    operator ignored `.lazy/` or `.lazy/wt/`. (Checking the bare top component
    //    `.lazy` would miss a `.lazy/` rule — git's trailing-slash rule only matches
    //    the path when it resolves to a directory.)
    let state_dir = worktree_root
        .split('/')
        .find(|c| !c.is_empty())
        .unwrap_or(worktree_root);
    let ignored = match git(repo, &["check-ignore", "-q", worktree_root]).await {
        Ok(o) => o.ok, // exit 0 ⇒ the path is ignored
        Err(_) => false,
    };
    if !ignored {
        problems.push(format!(
            "the daemon writes its db and every task worktree under {repo}/{state_dir}/, but `{state_dir}/` is not gitignored — a task's `git add` will stage daemon state into its branch and corrupt the merge into `{base_branch}`. Add `{state_dir}/` to {repo}/.gitignore and commit it, then start.",
            repo = repo.display()
        ));
    }

    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn init_repo(dir: &Path) {
        git(dir, &["init", "-q"]).await.unwrap();
        git(dir, &["config", "user.email", "t@t"]).await.unwrap();
        git(dir, &["config", "user.name", "t"]).await.unwrap();
        std::fs::write(dir.join("README"), "x").unwrap();
        git(dir, &["add", "."]).await.unwrap();
        git(dir, &["commit", "-qm", "init"]).await.unwrap();
    }

    #[tokio::test]
    async fn missing_repo_is_blocked() {
        let p = workspace_preflight(Path::new("/no/such/repo"), "main", ".lazy/wt").await;
        assert_eq!(p.len(), 1);
        assert!(p[0].contains("does not exist"));
    }

    #[tokio::test]
    async fn not_a_git_repo_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let p = workspace_preflight(dir.path(), "main", ".lazy/wt").await;
        assert_eq!(p.len(), 1);
        assert!(p[0].contains("not a git work tree"));
    }

    #[tokio::test]
    async fn unignored_state_dir_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        // No .gitignore at all ⇒ .lazy is not ignored.
        let p = workspace_preflight(dir.path(), "main", ".lazy/wt").await;
        assert!(
            p.iter().any(|m| m.contains("not gitignored")),
            "expected the unignored-state-dir block, got: {p:?}"
        );
    }

    #[tokio::test]
    async fn clean_ignored_repo_passes() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        std::fs::write(dir.path().join(".gitignore"), ".lazy/\n").unwrap();
        git(dir.path(), &["add", ".gitignore"]).await.unwrap();
        git(dir.path(), &["commit", "-qm", "ignore"]).await.unwrap();
        let p = workspace_preflight(dir.path(), "main", ".lazy/wt").await;
        assert!(p.is_empty(), "a clean, .lazy-ignored repo should pass: {p:?}");
    }

    #[tokio::test]
    async fn dirty_working_tree_does_not_block() {
        // A dirty main checkout is normal once a workflow is running (landing merges
        // move HEAD); it must not block start.
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path()).await;
        std::fs::write(dir.path().join(".gitignore"), ".lazy/\n").unwrap();
        git(dir.path(), &["add", ".gitignore"]).await.unwrap();
        git(dir.path(), &["commit", "-qm", "ignore"]).await.unwrap();
        std::fs::write(dir.path().join("README"), "changed").unwrap();
        let p = workspace_preflight(dir.path(), "main", ".lazy/wt").await;
        assert!(p.is_empty(), "a dirty tree must not block start: {p:?}");
    }
}
