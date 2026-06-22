//! Compose the agent prompt from the task's stored spec plus a fixed charter.
//!
//! The spec comes from the DB (`task.spec`) — never re-read from `tasks/*.md` at
//! runtime (SCOPE.md principle 6). The charter tells the agent, in order: where
//! it works, to leave its work **uncommitted** (the engine commits + pushes each
//! task as exactly one commit on the green gate — the agent must NOT commit or push
//! itself, or two tasks' work would blur on a shared branch), to signal
//! DONE/BLOCKED on its hcom thread exactly once, to reply to the operator on that
//! thread, and to stay inside its worktree.
//!
//! When the task carries a chat history (an operator workshopping a revived task,
//! or steering one mid-flight), the conversation is appended so a re-spawned agent
//! resumes with the operator's guidance in view rather than starting cold.

use lazybones_store::{ChatMessage, ChatRole, Task};

/// Build the full prompt for `task` working in `worktree` on `branch`, pushing to
/// `remote`, with the prior operator conversation (empty for a first run).
#[must_use]
pub fn compose(
    task: &Task,
    worktree: &str,
    branch: &str,
    remote: &str,
    history: &[ChatMessage],
) -> String {
    let id = &task.id;
    let _ = remote; // the engine pushes; the agent no longer runs git push
    let base = format!(
        "You are working in `{worktree}` on branch `{branch}`. Implement the task below.\n\
         \n\
         Make your changes in the worktree and leave them UNCOMMITTED. Do NOT run\n\
         `git commit` or `git push` — the engine commits your work as a single\n\
         commit for this task and pushes it once the build gate passes. Committing\n\
         yourself would blur this task's commit with the next one on a shared branch.\n\
         \n\
         When the implementation is complete:\n\
         1. Signal completion exactly once on the hcom thread named `{id}`:\n\
         `hcom send @all --thread {id} -- DONE`\n\
         (or `hcom send @all --thread {id} -- BLOCKED: <reason>` if you cannot finish).\n\
         2. Then stop.\n\
         \n\
         If the operator messages you on the thread `{id}`, reply to them on that\n\
         same thread (`hcom send @all --thread {id} -- <your reply>`), act on their\n\
         guidance, and signal DONE/BLOCKED again when you reach a new conclusion.\n\
         \n\
         Do not touch files outside this worktree. In particular, do NOT write\n\
         memory notes or edit anything under `~/.claude` or any `.claude/` metadata\n\
         directory — if you would normally save a memory, skip it silently and keep\n\
         working. Never stop to ask for permission; if an action would require\n\
         approval, skip it and continue.\n\
         \n\
         === TASK: {title} ===\n\
         {spec}\n",
        title = task.title,
        spec = task.spec,
    );
    if history.is_empty() {
        return base;
    }
    format!("{base}\n{}\n", conversation_section(id, history))
}

/// Render the prior operator conversation as a prompt section. Only called when
/// the history is non-empty (a revived or actively-steered task).
fn conversation_section(id: &str, history: &[ChatMessage]) -> String {
    let mut out = String::from(
        "=== OPERATOR CONVERSATION ===\n\
         This task was attempted before and the operator has been in touch. Read the\n\
         exchange below (oldest first), address the operator's latest guidance,\n\
         leave your work uncommitted (the engine commits + pushes), then re-signal\n\
         DONE/BLOCKED on the thread as above.\n\n",
    );
    for m in history {
        let who = match m.role {
            ChatRole::User => "operator",
            ChatRole::Agent => "you",
        };
        out.push_str(&format!("[{who}] {}\n", m.text.trim()));
    }
    let _ = id; // thread id already stated in the charter above
    out
}

#[cfg(test)]
mod tests {
    use super::compose;
    use lazybones_store::{ChatMessage, ChatRole, Task};

    fn msg(role: ChatRole, text: &str) -> ChatMessage {
        ChatMessage {
            run: "wf".into(),
            task: "auth".into(),
            role,
            text: text.into(),
            at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn includes_charter_and_spec() {
        let task = Task::seed(
            "auth",
            "run",
            "Add auth",
            "Build the login.",
            vec![],
            vec![],
            None,
        );
        let p = compose(&task, "/wt/auth", "lazy/auth", "origin", &[]);
        assert!(p.contains("/wt/auth"));
        assert!(p.contains("lazy/auth"));
        // The engine owns commit + push now; the agent is told NOT to.
        assert!(p.contains("UNCOMMITTED"));
        assert!(p.contains("Do NOT run"));
        assert!(!p.contains("git push origin lazy/auth"));
        assert!(p.contains("--thread auth -- DONE"));
        assert!(p.contains("Build the login."));
        // No conversation section on a first run.
        assert!(!p.contains("OPERATOR CONVERSATION"));
        // The charter forbids the out-of-worktree memory write that parks a
        // headless agent on the protected-`.claude/` approval prompt.
        assert!(p.contains(".claude"));
        assert!(p.to_lowercase().contains("memory"));
    }

    #[test]
    fn folds_in_the_operator_conversation_when_present() {
        let task = Task::seed(
            "auth",
            "run",
            "Add auth",
            "Build the login.",
            vec![],
            vec![],
            None,
        );
        let history = vec![
            msg(
                ChatRole::User,
                "the test fails because the port is hardcoded",
            ),
            msg(ChatRole::Agent, "got it, switching to an env var"),
        ];
        let p = compose(&task, "/wt/auth", "lazy/auth", "origin", &history);
        assert!(p.contains("OPERATOR CONVERSATION"));
        assert!(p.contains("[operator] the test fails because the port is hardcoded"));
        assert!(p.contains("[you] got it, switching to an env var"));
        // The spec/charter are still present.
        assert!(p.contains("Build the login."));
    }
}
