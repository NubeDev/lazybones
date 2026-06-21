//! The management-agent chat runner — one conversational turn.
//!
//! Off the scheduler loop (`docs/agent/lazybones-agent-scope.md` §3.2): loads the
//! config, builds the system prompt, spawns/resumes an hcom session for the
//! configured tool, sends the operator's turn, waits for the agent's reply, and
//! mirrors it into the durable conversation (which publishes it on the live bus
//! for the per-conversation SSE stream). The agent does its REST work itself with
//! the scoped token + base URL handed in by the caller (minted API-side so it can
//! register in the token table; never carries `Claim`/`Secret`, §10).

use std::time::Duration;

use lazybones_store::{
    AgentRole, ManagementAgentConfig, ManagementAgentScope, SessionMode, Skill, StoreHandle,
};

use crate::hcom::{AgentLaunch, DAEMON_SENDER, Hcom};

use super::prompt;

/// How long to wait for the agent's reply on the conversation thread before
/// giving up this turn (the agent may be doing several REST round-trips).
const REPLY_TIMEOUT: Duration = Duration::from_secs(180);

/// Everything one turn needs that the engine cannot derive on its own: the
/// scoped token + base URL the agent calls the REST API with, and the rendered
/// page-context ground-truth line.
#[derive(Debug, Clone)]
pub struct TurnContext {
    /// The minted scoped bearer token (Author/ReadOnly — never Claim/Secret).
    pub token: String,
    /// The base URL of this lazybonesd's REST API (e.g. `http://127.0.0.1:8080`).
    pub base_url: String,
    /// The rendered page-context line, or empty for a global conversation.
    pub page_context: String,
    /// The workflow id this conversation is scoped to (from the page context),
    /// if any — selects a per-workflow config override (scope §11 Q1).
    pub workflow_scope: Option<String>,
}

/// Run one conversational turn for `conversation_id`: deliver `user_text` to the
/// agent and mirror its reply into the store.
///
/// The operator's turn is persisted by the caller (the API route) *before* this
/// runs, so it is already in `history`/durable. This function spawns/resumes the
/// session, sends the turn, awaits the reply, and appends it as an
/// [`AgentRole::Agent`] message — which publishes on the bus for SSE.
///
/// # Errors
/// Returns an error if the config is unset, the tool cannot be spawned, or hcom
/// cannot be driven. A reply timeout is **not** an error — it appends a note and
/// returns `Ok`, leaving the conversation re-runnable.
pub async fn chat_turn(
    store: &StoreHandle,
    conversation_id: &str,
    user_text: &str,
    turn: &TurnContext,
) -> anyhow::Result<()> {
    // Resolve the effective config for this conversation's scope: a per-workflow
    // override if one exists, else the global config (scope §11 Q1).
    let scope = match &turn.workflow_scope {
        Some(id) => ManagementAgentScope::Workflow(id.clone()),
        None => ManagementAgentScope::Global,
    };
    let config = store
        .get_management_agent_resolved(&scope)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the Lazybones Agent is not configured yet"))?;

    let hcom = build_hcom(store, turn).await;

    // The conversation so far (oldest first) — the user turn the route just
    // persisted is included, so a fresh per-turn spawn resumes with full context.
    let history = store.agent_message_history(conversation_id).await?;
    let skills = enabled_skills(store, &config).await;

    let system_prompt = prompt::compose(
        conversation_id,
        &turn.base_url,
        &skills,
        &turn.page_context,
        &history,
        config.permission_profile.can_manage(),
    );

    // Defensively drop `--dangerously-skip-permissions`: the management agent
    // runs in a scratch dir with a bootstrapped Claude allow-list, so it never
    // needs bypass mode — and that flag triggers a consent screen a headless
    // agent can't answer, stalling it `launch_blocked`. Filtering here (not just
    // the empty default) means a stored/edited config carrying the flag can't
    // silently break the agent (`docs/agent/lazybones-agent-scope.md`).
    let perm_flags: Vec<String> = config
        .permission_flags
        .iter()
        .filter(|f| !is_bypass_flag(f))
        .cloned()
        .collect();
    let launch = AgentLaunch {
        model: config.model.as_deref(),
        effort: config.effort.as_deref(),
        permission_flags: &perm_flags,
    };

    let session_live = matches!(config.session_mode, SessionMode::PerConversation)
        && session_exists(&hcom, conversation_id).await;

    if config.session_mode == SessionMode::PerTurn || !session_live {
        // Spawn a fresh session for this conversation. The management agent has no
        // worktree — it works purely through REST — so it runs in a dedicated
        // scratch dir we own, bootstrapped with a Claude allow-list so a headless
        // `claude` doesn't stall on the folder-trust dialog (the same gate the
        // scheduler clears for task agents via the worktree `.claude/settings.json`).
        let dir = prepare_agent_dir()?;
        hcom.spawn(&config.tool, conversation_id, &dir, &system_prompt, launch)
            .await
            .inspect_err(|e| {
                tracing::warn!(
                    conversation = %conversation_id,
                    tool = %config.tool,
                    "management: agent spawn failed: {e}"
                );
            })?;
        // A freshly spawned agent already has the user turn in its prompt history,
        // so no separate send is needed on the spawn path.
    } else {
        // Resume: the live session keeps its context; just deliver the new turn.
        hcom.send(conversation_id, user_text).await?;
    }

    // Wait for the agent's reply on the conversation thread. Exclude the daemon's
    // own messages: delivering a follow-up turn via `--from lazybones` posts that
    // message onto the thread as `instance = 'ext_lazybones'`, and without this
    // filter the wait would return immediately with the operator's own words
    // echoed back as the "reply" instead of blocking for the agent's actual reply.
    // The sender is the top-level `instance` column in hcom's `events_v` view
    // (not a `data.$.from` JSON field).
    let ext_sender = format!("ext_{DAEMON_SENDER}");
    let sql = format!(
        "type = 'message' \
         AND json_extract(data, '$.thread') = '{conversation_id}' \
         AND instance != '{ext_sender}'"
    );
    let events = hcom.wait(&sql, REPLY_TIMEOUT).await.unwrap_or_default();

    // Belt-and-suspenders: skip any event still attributed to the daemon sender.
    let reply = events.iter().rev().find_map(|e| {
        if e.instance == ext_sender || e.instance == DAEMON_SENDER {
            return None;
        }
        e.data
            .get("text")
            .and_then(|t| t.as_str())
            .filter(|t| !t.trim().is_empty())
            .map(|t| (t.to_owned(), e.id_int()))
    });

    match reply {
        Some((text, hcom_id)) => {
            let parsed = super::confirm::parse_reply(&text);
            // Persist the prose reply (if any) so the operator sees the agent's
            // explanation alongside the confirm card.
            if !parsed.text.is_empty() {
                store
                    .append_agent_message(conversation_id, AgentRole::Agent, &parsed.text, hcom_id)
                    .await?;
            }
            // A gated action is only honoured when the profile allows it; an
            // agent on a read/author profile that tries to propose one gets it
            // surfaced as prose, never as an actionable card.
            if let Some((summary, action)) = parsed.confirm {
                if config.permission_profile.can_manage() {
                    store
                        .append_confirm_request(conversation_id, &summary, &action, hcom_id)
                        .await?;
                } else {
                    store
                        .append_agent_message(
                            conversation_id,
                            AgentRole::Tool,
                            &format!(
                                "(the agent proposed a lifecycle action — {summary} — but its \
                                 permission profile cannot manage; enable \"Author & manage\" to \
                                 act on it)"
                            ),
                            None,
                        )
                        .await?;
                }
            }
        }
        None => {
            store
                .append_agent_message(
                    conversation_id,
                    AgentRole::Tool,
                    "(the agent did not reply in time — send another message to continue)",
                    None,
                )
                .await?;
        }
    }

    Ok(())
}

/// Whether `flag` is a bypass-permissions flag that must not reach the headless
/// management agent (it triggers a consent screen the agent can't answer). Covers
/// the canonical spelling and its common short alias.
fn is_bypass_flag(flag: &str) -> bool {
    matches!(
        flag.trim(),
        "--dangerously-skip-permissions" | "--dangerously-skip-permission"
    )
}

/// The Claude allow-list written into the management agent's scratch dir so a
/// headless `claude` trusts the folder and skips the interactive trust dialog
/// (mirrors the scheduler's worktree bootstrap). Without it the agent is reaped
/// `launch_blocked: screen settled before readiness` and never replies.
const CLAUDE_SETTINGS_BOOTSTRAP: &str = r#"{
  "permissions": {
    "allow": ["Bash", "Edit", "Write", "Read", "Glob", "Grep", "WebFetch", "WebSearch", "Skill", "Task", "TodoWrite", "NotebookEdit"]
  }
}
"#;

/// Provision (idempotently) the scratch directory the management agent runs in:
/// `<cwd>/.lazy/agent`, with a bootstrapped `.claude/settings.json`. The agent
/// needs no repo — it works purely through REST — so a stable owned dir keeps the
/// daemon's working tree clean and clears the folder-trust gate once.
///
/// # Errors
/// Returns an error if the directory or settings file cannot be created.
fn prepare_agent_dir() -> anyhow::Result<std::path::PathBuf> {
    prepare_agent_dir_in(&std::env::current_dir()?)
}

/// `prepare_agent_dir` rooted at an explicit `base` (so it is unit-testable
/// without touching the process CWD).
fn prepare_agent_dir_in(base: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    let dir = base.join(".lazy").join("agent");
    let claude = dir.join(".claude");
    std::fs::create_dir_all(&claude)?;
    let settings = claude.join("settings.json");
    if !settings.exists() {
        std::fs::write(&settings, CLAUDE_SETTINGS_BOOTSTRAP)?;
    }
    Ok(dir)
}

/// Build the hcom client, exporting the agent CLI credentials plus the scoped
/// lazybones token + base URL the management agent calls the REST API with.
async fn build_hcom(store: &StoreHandle, turn: &TurnContext) -> Hcom {
    let mut env = match store.secret_env().await {
        Ok(pairs) => pairs.into_iter().map(|s| (s.env_var, s.value)).collect::<Vec<_>>(),
        Err(e) => {
            tracing::warn!("management: loading secret env failed, spawning without it: {e}");
            Vec::new()
        }
    };
    env.push(("LAZYBONES_TOKEN".to_owned(), turn.token.clone()));
    env.push(("LAZYBONES_BASE_URL".to_owned(), turn.base_url.clone()));
    Hcom::discover().with_env(env)
}

/// Resolve the operator-enabled skills into their full records (bodies folded
/// into the prompt). A missing/failed lookup is skipped, not fatal.
async fn enabled_skills(store: &StoreHandle, config: &ManagementAgentConfig) -> Vec<Skill> {
    let mut out = Vec::new();
    for id in &config.enabled_skills {
        match store.get_skill(id).await {
            Ok(Some(skill)) => out.push(skill),
            Ok(None) => tracing::warn!(skill = %id, "management: enabled skill not found, skipping"),
            Err(e) => tracing::warn!(skill = %id, "management: skill load failed: {e}"),
        }
    }
    out
}

/// Whether a live hcom session exists for `tag` (the conversation id).
async fn session_exists(hcom: &Hcom, tag: &str) -> bool {
    match hcom.list().await {
        Ok(agents) => agents
            .iter()
            .any(|a| a.tag.as_deref() == Some(tag) && a.status != "dead"),
        Err(e) => {
            tracing::warn!("management: hcom list failed, assuming no live session: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_flags_are_recognised_others_pass() {
        assert!(is_bypass_flag("--dangerously-skip-permissions"));
        assert!(is_bypass_flag("  --dangerously-skip-permissions  "));
        assert!(!is_bypass_flag("--model"));
        assert!(!is_bypass_flag("--verbose"));
    }

    #[test]
    fn prepare_agent_dir_writes_allowlist_and_is_idempotent() {
        let base = tempfile::tempdir().unwrap();
        let dir = prepare_agent_dir_in(base.path()).unwrap();
        let settings = dir.join(".claude").join("settings.json");
        assert!(settings.exists(), "settings.json must be written");
        let body = std::fs::read_to_string(&settings).unwrap();
        assert!(body.contains("\"Bash\""), "allow-list must grant Bash");

        // A second call must not error and must not clobber an existing file.
        std::fs::write(&settings, "{\"custom\":true}").unwrap();
        let again = prepare_agent_dir_in(base.path()).unwrap();
        assert_eq!(again, dir);
        let after = std::fs::read_to_string(&settings).unwrap();
        assert_eq!(after, "{\"custom\":true}", "existing settings preserved");
    }
}
