//! Compose the agent prompt from the task's stored spec plus a fixed charter.
//!
//! The spec comes from the DB (`task.spec`) — never re-read from `tasks/*.md` at
//! runtime (SCOPE.md principle 6). The charter tells the agent, in order: where
//! it works, to commit + push, to signal DONE/BLOCKED on its hcom thread exactly
//! once, and to stay inside its worktree.

use lazybones_store::Task;

/// Build the full prompt for `task` working in `worktree` on `branch`, pushing
/// to `remote`.
#[must_use]
pub fn compose(task: &Task, worktree: &str, branch: &str, remote: &str) -> String {
    let id = &task.id;
    format!(
        "You are working in `{worktree}` on branch `{branch}`. Implement the task below.\n\
         \n\
         When the implementation is complete:\n\
         1. Commit your work, then run `git push {remote} {branch}`.\n\
         2. Signal completion exactly once on the hcom thread named `{id}`:\n\
         `hcom send @all --thread {id} -- DONE`\n\
         (or `hcom send @all --thread {id} -- BLOCKED: <reason>` if you cannot finish).\n\
         3. Then stop.\n\
         \n\
         Do not touch files outside this worktree.\n\
         \n\
         === TASK: {title} ===\n\
         {spec}\n",
        title = task.title,
        spec = task.spec,
    )
}

#[cfg(test)]
mod tests {
    use super::compose;
    use lazybones_store::Task;

    #[test]
    fn includes_charter_and_spec() {
        let task = Task::seed("auth", "run", "Add auth", "Build the login.", vec![], vec![], None);
        let p = compose(&task, "/wt/auth", "lazy/auth", "origin");
        assert!(p.contains("/wt/auth"));
        assert!(p.contains("lazy/auth"));
        assert!(p.contains("git push origin lazy/auth"));
        assert!(p.contains("--thread auth -- DONE"));
        assert!(p.contains("Build the login."));
    }
}
