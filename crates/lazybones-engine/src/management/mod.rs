//! The Lazybones-Agent runner — a conversational operator aide off the scheduler
//! loop (`docs/agent/lazybones-agent-scope.md` §3.2).
//!
//! It authors workflows/tasks/templates/skills and explains state through the
//! same REST API a human uses; it never starts or runs anything. A turn loads the
//! config, builds the system prompt (persona + enabled skill runbooks + page
//! context + REST cheat-sheet), spawns/resumes an hcom session, and streams the
//! reply back via the store's live bus.

mod cheatsheet;
mod confirm;
mod context;
mod prompt;
mod runner;

pub use context::{render as render_page_context, workflow_id as page_context_workflow_id};
pub use runner::{TurnContext, chat_turn};

#[cfg(test)]
mod tests {
    use super::context::render;

    #[test]
    fn render_empty_for_none() {
        assert_eq!(render(None), "");
    }

    #[test]
    fn renders_workflow_context() {
        let v = serde_json::json!({
            "view": "workflows",
            "workflow_id": "add-healthcheck",
            "repo": "/repo/foo",
            "base_branch": "main"
        });
        let line = render(Some(&v));
        assert!(line.contains("`workflows`"));
        assert!(line.contains("add-healthcheck"));
        assert!(line.contains("/repo/foo"));
        assert!(line.contains("main"));
    }

    #[test]
    fn renders_task_context() {
        let v = serde_json::json!({ "view": "tasks", "task_id": "impl", "run_id": "wf-1" });
        let line = render(Some(&v));
        assert!(line.contains("task `impl`"));
        assert!(line.contains("`wf-1`"));
    }
}
