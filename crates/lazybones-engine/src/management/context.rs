//! Render the UI's page-context envelope into a ground-truth prompt line.
//!
//! The UI sends a small typed envelope with every turn (the page the operator is
//! viewing + any ids in scope, `docs/agent/lazybones-agent-scope.md` §7). The
//! runner renders it as ground truth ("The operator is currently viewing workflow
//! `X`."). Ids are hints — the agent still re-reads authoritative state via GET.
//!
//! The envelope is opaque JSON at the store boundary; this is the one place that
//! knows its shape, kept lenient (every field optional) so a new UI field is
//! additive.

use serde::Deserialize;

/// The page-context envelope, mirrored from `ui/src/types/page-context.ts`.
#[derive(Debug, Default, Deserialize)]
pub struct PageContext {
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub workflow_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub base_branch: Option<String>,
    #[serde(default)]
    pub selected_template_id: Option<String>,
    #[serde(default)]
    pub selected_skill_id: Option<String>,
}

/// Extract the workflow id a page context is scoped to, if any — used to select
/// a per-workflow config override (scope §11 Q1).
#[must_use]
pub fn workflow_id(value: Option<&serde_json::Value>) -> Option<String> {
    let ctx: PageContext = serde_json::from_value(value?.clone()).ok()?;
    ctx.workflow_id
}

/// Render a page-context JSON value into a human-readable ground-truth line.
/// Returns an empty string for a global (no-context) conversation or unparsable
/// input — the prompt then omits the page-context section entirely.
#[must_use]
pub fn render(value: Option<&serde_json::Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let ctx: PageContext = match serde_json::from_value(value.clone()) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let mut parts = Vec::new();
    if let Some(view) = &ctx.view {
        parts.push(format!("The operator is currently on the `{view}` page."));
    }
    if let Some(wf) = &ctx.workflow_id {
        let mut line = format!("They are viewing workflow `{wf}`");
        if let Some(repo) = &ctx.repo {
            line.push_str(&format!(" in repo `{repo}`"));
        }
        if let Some(branch) = &ctx.base_branch {
            line.push_str(&format!(" (base branch `{branch}`)"));
        }
        line.push('.');
        parts.push(line);
    }
    if let Some(task) = &ctx.task_id {
        parts.push(format!("They are viewing task `{task}`."));
    }
    if let Some(run) = &ctx.run_id {
        parts.push(format!("The run in scope is `{run}`."));
    }
    if let Some(t) = &ctx.selected_template_id {
        parts.push(format!("The selected template is `{t}`."));
    }
    if let Some(s) = &ctx.selected_skill_id {
        parts.push(format!("The selected skill is `{s}`."));
    }

    parts.join(" ")
}
