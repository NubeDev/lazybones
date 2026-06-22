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

    // Pin a cursor at the newest event *before* we spawn/send, so the incremental
    // drain below sees only this turn's output and never replays the conversation's
    // backlog. A failure here is non-fatal — we fall back to a cursor of 0 and the
    // daemon/thread filters still keep us to fresh, agent-authored messages.
    let mut cursor = hcom.latest_event_id().await.unwrap_or(0);

    if config.session_mode == SessionMode::PerTurn || !session_live {
        // Spawn a fresh session for this conversation. The management agent has no
        // worktree — it works purely through REST — so it runs in a dedicated
        // scratch dir we own, bootstrapped with a Claude allow-list (clears the
        // per-tool approval gate). The folder-trust dialog is a *separate* gate the
        // allow-list does NOT cover, so for `claude` we also pre-seed the scratch
        // dir's `hasTrustDialogAccepted` in `~/.claude.json` — without it a headless
        // `claude` freezes on *"Yes, I trust this folder"* and is reaped
        // `launch_blocked`. The bypass flag is filtered out (it drags in its own
        // consent screen), so trust-seeding is the only thing keeping the agent
        // unblocked on a fresh host.
        let dir = prepare_agent_dir()?;
        if config.tool == "claude" {
            crate::hcom::seed_claude_folder_trust(&dir);
        }
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

    // Live feedback (Problem 1): the agent may spend 5–180s doing REST round-trips
    // before it replies. Incrementally drain the thread so each agent message lands
    // on the conversation (and the SSE feed) the moment it's posted, AND emit
    // ephemeral "Running a command…" activity ticks from the agent's tool-status
    // events — so the operator sees live progress, not a blank spinner. Durable
    // history gets only the messages; the activity ticks are transient. The agent's
    // hcom base name (which attributes its status events) is resolved lazily inside
    // the loop, since a freshly spawned agent isn't in `hcom list` immediately.
    stream_replies(store, &hcom, conversation_id, &config, &mut cursor).await;

    Ok(())
}

/// Resolve the hcom base name of the agent spawned for `tag` (the conversation
/// id), e.g. `kale` for `ecksj…-kale`. Tool-status events are attributed to this
/// base name. `None` if no live agent is found (activity streaming is then a
/// no-op; messages still stream by thread).
async fn agent_base_name(hcom: &Hcom, tag: &str) -> Option<String> {
    match hcom.list().await {
        Ok(agents) => agents
            .into_iter()
            .find(|a| a.tag.as_deref() == Some(tag) && a.status != "dead")
            .map(|a| a.base_name),
        Err(e) => {
            tracing::warn!("management: hcom list for base name failed: {e}");
            None
        }
    }
}

/// A short human-readable note for a tool-status `context` like `tool:Bash`.
fn activity_note(context: &str) -> Option<String> {
    let tool = context.strip_prefix("tool:")?;
    let verb = match tool {
        "Bash" => "Running a command",
        "Read" => "Reading a file",
        "Edit" | "Write" => "Writing a file",
        "Glob" | "Grep" => "Searching",
        "WebFetch" | "WebSearch" => "Looking something up",
        "Task" => "Working",
        other => return Some(format!("Using {other}…")),
    };
    Some(format!("{verb}…"))
}

/// How often to drain the conversation thread while waiting for the agent. Short
/// enough that a reply (or an intermediate message) reaches the panel promptly,
/// long enough not to hammer the `hcom` binary.
const POLL_INTERVAL: Duration = Duration::from_millis(1200);

/// Drain the conversation thread incrementally until the agent has replied and
/// then gone quiet, or [`REPLY_TIMEOUT`] elapses. Every fresh agent message is
/// streamed onto the durable conversation (and thus the SSE feed) as it arrives —
/// so the operator sees progress, not one final dump after a long pause.
///
/// `cursor` is advanced past every event consumed, so re-polls never re-emit. A
/// timeout with no reply appends a re-runnable note rather than erroring.
async fn stream_replies(
    store: &StoreHandle,
    hcom: &Hcom,
    conversation_id: &str,
    config: &ManagementAgentConfig,
    cursor: &mut u64,
) {
    let ext_sender = format!("ext_{DAEMON_SENDER}");
    let deadline = tokio::time::Instant::now() + REPLY_TIMEOUT;
    // Once the first real agent message lands the turn is essentially answered; we
    // poll one more interval to catch any trailing message, then stop — rather
    // than blocking out the full timeout.
    let mut replied = false;
    // Coalesce repeated identical activity ticks (claude fires several `tool:Bash`
    // status events per call) so we don't spam the panel with the same line.
    let mut last_activity: Option<String> = None;
    // The agent's hcom base name, resolved lazily (a freshly spawned agent isn't
    // in `hcom list` immediately). Until known, status-event attribution is skipped.
    let mut agent_base: Option<String> = None;
    // Whether we've ever seen the agent alive — so a later "no longer alive" means
    // it was stopped (by the operator's Stop) or crashed, not just slow to appear.
    let mut was_alive = false;

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;

        if agent_base.is_none() {
            agent_base = agent_base_name(hcom, conversation_id).await;
        }

        // If the agent was alive and is now gone, the turn is over — most likely
        // the operator pressed Stop (`POST /agent/chat/:id/stop` → kill the tag).
        // End the wait promptly instead of holding the session for the full
        // timeout. The stop route itself records the "stopped" note.
        let alive = session_exists(hcom, conversation_id).await;
        if alive {
            was_alive = true;
        } else if was_alive {
            return;
        }

        let events = match hcom.events_since(*cursor).await {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!(conversation = %conversation_id, "management: drain failed: {e}");
                continue;
            }
        };

        let mut saw_new = false;
        for e in &events {
            if let Some(id) = e.id_int().and_then(|id| u64::try_from(id).ok()) {
                *cursor = (*cursor).max(id);
            }

            // Stream ephemeral activity from the agent's tool-status events
            // ("Running a command…"), attributed by base name. Not persisted.
            if e.kind == "status"
                && agent_base.as_deref().is_some_and(|b| e.instance == b)
                && let Some(context) = e.data.get("context").and_then(|c| c.as_str())
                && let Some(note) = activity_note(context)
            {
                if last_activity.as_deref() != Some(note.as_str()) {
                    store.report_agent_activity(conversation_id, &note);
                    last_activity = Some(note);
                }
                continue;
            }

            // Only the agent's own messages on *this* thread; skip the daemon's
            // own `--from lazybones` echoes (top-level `instance` column).
            if e.kind != "message" || e.instance == ext_sender || e.instance == DAEMON_SENDER {
                continue;
            }
            if e.data.get("thread").and_then(|t| t.as_str()) != Some(conversation_id) {
                continue;
            }
            let Some(text) = e
                .data
                .get("text")
                .and_then(|t| t.as_str())
                .filter(|t| !t.trim().is_empty())
            else {
                continue;
            };
            saw_new = true;
            replied = true;
            persist_reply(store, conversation_id, config, text, e.id_int()).await;
        }

        // After the agent has answered, one quiet poll (no new messages) ends the
        // turn — no need to hold the session open until the full timeout.
        if replied && !saw_new {
            return;
        }
    }

    if !replied {
        let _ = store
            .append_agent_message(
                conversation_id,
                AgentRole::Tool,
                "(the agent did not reply in time — send another message to continue)",
                None,
            )
            .await;
    }
}

/// Persist one agent message: its prose as an [`AgentRole::Agent`] reply, and a
/// `CONFIRM:` line (managed profile only) as a gated confirm request — otherwise
/// surfaced as prose so a read/author agent can't smuggle an actionable card.
async fn persist_reply(
    store: &StoreHandle,
    conversation_id: &str,
    config: &ManagementAgentConfig,
    text: &str,
    hcom_id: Option<i64>,
) {
    let parsed = super::confirm::parse_reply(text);
    if !parsed.text.is_empty()
        && let Err(e) = store
            .append_agent_message(conversation_id, AgentRole::Agent, &parsed.text, hcom_id)
            .await
    {
        tracing::warn!(conversation = %conversation_id, "management: persist reply failed: {e}");
    }
    if let Some((summary, action)) = parsed.confirm {
        let result = if config.permission_profile.can_manage() {
            store
                .append_confirm_request(conversation_id, &summary, &action, hcom_id)
                .await
                .map(|_| ())
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
                .await
                .map(|_| ())
        };
        if let Err(e) = result {
            tracing::warn!(conversation = %conversation_id, "management: persist confirm failed: {e}");
        }
    }
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
/// headless `claude` clears the per-tool **approval** gate (mirrors the scheduler's
/// worktree bootstrap). This is distinct from the folder-**trust** gate, which the
/// caller clears separately via [`crate::hcom::seed_claude_folder_trust`]. Without
/// both, the agent is reaped `launch_blocked: screen settled before readiness`.
const CLAUDE_SETTINGS_BOOTSTRAP: &str = r#"{
  "permissions": {
    "allow": ["Bash", "Edit", "Write", "Read", "Glob", "Grep", "WebFetch", "WebSearch", "Skill", "Task", "TodoWrite", "NotebookEdit"]
  }
}
"#;

/// Provision (idempotently) the scratch directory the management agent runs in:
/// `<cwd>/.lazy/agent`, with a bootstrapped `.claude/settings.json` (the per-tool
/// approval allow-list). The agent needs no repo — it works purely through REST —
/// so a stable owned dir keeps the daemon's working tree clean. The folder-trust
/// gate is cleared by the caller via `seed_claude_folder_trust`, not here.
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
        Ok(pairs) => pairs
            .into_iter()
            .map(|s| (s.env_var, s.value))
            .collect::<Vec<_>>(),
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
            Ok(None) => {
                tracing::warn!(skill = %id, "management: enabled skill not found, skipping")
            }
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
