//! Compose the management agent's system prompt.
//!
//! System prompt = persona + the enabled skill runbooks + the page context +
//! the REST cheat-sheet + the prior conversation + a fixed reply charter
//! (`docs/agent/lazybones-agent-scope.md` §3.2, §6, §7). The agent reads the
//! skills as its operating procedures and executes the REST calls itself.

use lazybones_store::{AgentMessage, AgentRole, Skill, SkillAction};

use super::cheatsheet::cheatsheet;

/// The fixed persona + guardrail that opens every management-agent prompt.
const PERSONA: &str = "\
You are the Lazybones Agent: a conversational assistant embedded in the lazybones\n\
web UI. Your job is to help the operator MANAGE lazybones itself — author\n\
workflows, tasks, templates, and skills; explain current state; and supervise\n\
runs. You act ONLY through the lazybones REST API (below), exactly as a human\n\
operator would. You have no other privileged access.\n\
\n\
THE ONE RULE: you AUTHOR, the human STARTS. Creating a workflow or task is safe\n\
and unattended. Starting, stopping, retrying, cancelling, or deleting work is a\n\
human action you never take — your token cannot reach those endpoints. After you\n\
author something, end by telling the operator exactly what to press (e.g. \"Press\n\
Start when you're ready to run it.\").\n\
\n\
ALWAYS CONFIRM YOUR WORK: every turn ends with exactly one reply to the operator\n\
on your thread — no silent successes. When you create or edit anything (a\n\
workflow, task, template, or skill), your reply MUST say what you did and name\n\
it, e.g. \"Created template `healthcheck-test`.\" or \"Added task `impl` to\n\
workflow `add-healthcheck`.\" If a call fails (e.g. a 409 duplicate id), say so.\n\
Never finish a turn having taken an action without reporting it — a created\n\
template the operator can't see reported looks like nothing happened.\n";

/// The gated-action section, included only for the `AuthorAndManage` profile.
/// It tells the agent how to PROPOSE a lifecycle action without taking it.
fn manage_section(thread: &str) -> String {
    format!(
        "=== PROPOSING LIFECYCLE ACTIONS (managed profile) ===\n\
         You may PROPOSE lifecycle actions (start/stop/resume/restart a workflow;\n\
         retry/cancel a task; delete a workflow/task/template/skill), but you must\n\
         NEVER call those endpoints yourself — your token cannot, and the human\n\
         must confirm every one.\n\
         \n\
         To propose one, reply on the thread `{thread}` with a single line of the\n\
         exact form (and nothing after it on that line):\n\
         CONFIRM: {{\"action\":\"<verb>\",\"method\":\"<POST|PUT|DELETE>\",\"path\":\"<rest path>\",\"body\":<json-or-omit>}}\n\
         \n\
         Examples:\n\
         CONFIRM: {{\"action\":\"start\",\"method\":\"POST\",\"path\":\"/workflows/add-healthcheck/start\"}}\n\
         CONFIRM: {{\"action\":\"retry\",\"method\":\"POST\",\"path\":\"/tasks/impl/retry\"}}\n\
         \n\
         Put a one-sentence explanation BEFORE the CONFIRM line so the operator\n\
         knows why. The UI renders a Confirm/Cancel card and, on Confirm, makes the\n\
         call under the operator's own authority — not yours. Propose at most one\n\
         action per turn.\n"
    )
}

/// The reply charter: how the agent talks back on its conversation thread.
fn reply_charter(thread: &str) -> String {
    format!(
        "=== HOW TO REPLY ===\n\
         The operator is talking to you on the hcom thread named `{thread}`. When you\n\
         have an answer (or have finished authoring), reply to them on that same\n\
         thread exactly once per turn:\n\
         `hcom send @all --thread {thread} -- <your reply>`\n\
         Keep replies concise and operator-facing. Do REST work first, then ALWAYS\n\
         reply with what you did — name every workflow/task/template/skill you\n\
         created or edited — and what they should do next. Sending this reply is not\n\
         optional: a turn that authored something but did not report it back reads\n\
         to the operator as a failure. Do not signal DONE/BLOCKED — you are a\n\
         conversational aide, not a task agent.\n"
    )
}

/// Build the full system prompt for one conversation turn.
///
/// `thread` is the hcom thread (the conversation id). `enabled` are the skill
/// runbooks the operator turned on. `page_context` is the rendered ground-truth
/// line (empty when global). `history` is the prior conversation, oldest first.
#[must_use]
pub fn compose(
    thread: &str,
    base_url: &str,
    enabled: &[Skill],
    page_context: &str,
    history: &[AgentMessage],
    can_manage: bool,
) -> String {
    let mut out = String::from(PERSONA);
    out.push('\n');

    if can_manage {
        out.push_str(&manage_section(thread));
        out.push('\n');
    }

    if !page_context.is_empty() {
        out.push_str("=== PAGE CONTEXT ===\n");
        out.push_str(page_context);
        out.push_str(
            "\nThese ids are hints — prefer them as defaults but re-read authoritative\n\
             state with a GET before you act on them.\n\n",
        );
    }

    out.push_str(&cheatsheet(base_url));
    out.push('\n');

    if !enabled.is_empty() {
        out.push_str("=== YOUR SKILLS (operating runbooks) ===\n");
        for skill in enabled {
            out.push_str(&format!(
                "\n--- skill: {} ({}) ---\n",
                skill.id, skill.title
            ));
            out.push_str(&skill.body);
            out.push('\n');
            if let Some(action) = &skill.action {
                out.push_str(&render_action(&skill.id, action));
            }
        }
        out.push('\n');
    }

    if !history.is_empty() {
        out.push_str(&conversation_section(history));
        out.push('\n');
    }

    out.push_str(&reply_charter(thread));
    out
}

/// Render a skill's structured action as an explicit, deterministic procedure.
/// The agent is told exactly which call the action makes and which params to
/// gather — the typed counterpart to the prose runbook (scope §6.1, OQ2).
fn render_action(skill_id: &str, action: &SkillAction) -> String {
    let mut out = String::from("  [structured action — deterministic]\n");
    out.push_str(&format!(
        "  This skill has a typed action: {} {}\n",
        action.method, action.path_template
    ));
    if let Some(body) = &action.body_template {
        out.push_str(&format!("  Body template: {body}\n"));
    }
    if !action.params.is_empty() {
        out.push_str("  Parameters (substitute as {name} in the path/body):\n");
        for p in &action.params {
            let req = if p.required { "required" } else { "optional" };
            out.push_str(&format!("    - {} ({req}): {}\n", p.name, p.description));
        }
    }
    out.push_str(&format!(
        "  Gather the required params, fill the templates, and make exactly that\n\
         call (`{skill_id}`). Prefer this over improvising the request shape.\n"
    ));
    out
}

/// Render the prior conversation as a prompt section (oldest first).
fn conversation_section(history: &[AgentMessage]) -> String {
    let mut out = String::from(
        "=== CONVERSATION SO FAR ===\n\
         The exchange below is this conversation's history (oldest first). Continue it.\n\n",
    );
    for m in history {
        let who = match m.role {
            AgentRole::User => "operator",
            AgentRole::Agent => "you",
            AgentRole::Tool => "action",
            AgentRole::Confirm => "you proposed",
        };
        out.push_str(&format!("[{who}] {}\n", m.text));
    }
    out
}
