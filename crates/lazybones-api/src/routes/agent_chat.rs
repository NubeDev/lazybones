//! The global Lazybones-Agent chat surface (`docs/agent/lazybones-agent-scope.md` §3.2, §8).
//!
//! - `POST /agent/chat` — submit an operator turn (+ page context). Persists the
//!   turn, mints a scoped token, and drives one conversational turn off-request;
//!   the agent's reply arrives on the per-conversation SSE stream.
//! - `GET  /agent/chat/:conversation` — fetch persisted history.
//! - `GET  /agent/conversations` — list conversations.
//! - `GET  /agent/chat/:conversation/stream` — per-conversation SSE of messages.
//!
//! GUARDRAIL (§9): this surface only authors and reads. The minted token is
//! `Author`/`ReadOnly` (never `Claim`/`Secret`), and there is no path here to
//! start/stop/retry/delete — lifecycle is Phase 2.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::Sse;
use axum::response::sse::{Event as SseEvent, KeepAlive};
use lazybones_auth::ManagementProfile;
use lazybones_store::{
    AgentConversation, AgentMessage, AgentRole, LiveEvent, ManagementAgentScope, PermissionProfile,
};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{Stream, StreamExt};

use crate::dto::{AgentChatBody, AgentChatPosted};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// `POST /agent/chat` — submit one operator turn. Open like the task chat (a
/// local single-user daemon); the agent's *own* actions are bounded by its
/// scoped token, not this endpoint.
///
/// Resolves the conversation (creating one if `conversation` is absent),
/// persists the operator turn, then spawns the management runner off-request.
/// Returns immediately with the conversation id + the stored user message; the
/// agent's reply streams in over `/agent/chat/:conversation/stream`.
pub async fn post_agent_chat(
    State(state): State<AppState>,
    Json(body): Json<AgentChatBody>,
) -> ApiResult<Json<AgentChatPosted>> {
    let text = body.text.trim();
    if text.is_empty() {
        return Err(ApiError::bad_request("message text must not be empty"));
    }

    // Resolve or open the conversation, snapshotting the page context on open.
    let conversation = match &body.conversation {
        Some(id) => state
            .store
            .get_agent_conversation(id)
            .await?
            .ok_or(ApiError::NotFound)?,
        None => {
            state
                .store
                .create_agent_conversation(body.page_context.as_ref())
                .await?
        }
    };

    // Persist the operator turn first (durable + published on the bus for SSE).
    let message = state
        .store
        .append_agent_message(&conversation.id, AgentRole::User, text, None)
        .await?;

    // The effective page context for this turn (request envelope wins, else the
    // conversation's opening snapshot) and the workflow scope it implies.
    let ctx = body
        .page_context
        .as_ref()
        .or(conversation.page_context.as_ref());
    let workflow_scope = lazybones_engine::page_context_workflow_id(ctx);
    let scope = match &workflow_scope {
        Some(id) => ManagementAgentScope::Workflow(id.clone()),
        None => ManagementAgentScope::Global,
    };

    // Resolve the config for this scope (override ?? global) to learn the
    // permission profile, then mint a scoped token from it.
    let config = state
        .store
        .get_management_agent_resolved(&scope)
        .await?
        .unwrap_or_default();
    let token =
        state.mint_management_token(&conversation.id, auth_profile(config.permission_profile));

    let page_context = lazybones_engine::render_page_context(ctx);

    // Drive the turn off-request: the runner streams the reply back via the bus.
    let store = state.store.clone();
    let conversation_id = conversation.id.clone();
    let turn = lazybones_engine::TurnContext {
        token,
        base_url: state.base_url.clone(),
        page_context,
        workflow_scope,
    };
    let user_text = text.to_owned();
    tokio::spawn(async move {
        if let Err(e) =
            lazybones_engine::chat_turn(&store, &conversation_id, &user_text, &turn).await
        {
            tracing::warn!(conversation = %conversation_id, "agent chat turn failed: {e}");
            // Surface the failure into the conversation so the UI isn't left hanging.
            let _ = store
                .append_agent_message(
                    &conversation_id,
                    AgentRole::Tool,
                    &format!("(the agent could not complete this turn: {e})"),
                    None,
                )
                .await;
        }
    });

    Ok(Json(AgentChatPosted {
        conversation: conversation.id,
        message,
    }))
}

/// `GET /agent/chat/:conversation` — the conversation's messages, oldest first.
/// `404` if the conversation is unknown.
pub async fn get_agent_chat(
    State(state): State<AppState>,
    Path(conversation): Path<String>,
) -> ApiResult<Json<Vec<AgentMessage>>> {
    state
        .store
        .get_agent_conversation(&conversation)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(
        state.store.agent_message_history(&conversation).await?,
    ))
}

/// `GET /agent/conversations` — list conversations, newest first.
pub async fn list_agent_conversations(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<AgentConversation>>> {
    Ok(Json(state.store.list_agent_conversations().await?))
}

/// `POST /agent/chat/:conversation/stop` — stop the agent currently running this
/// conversation's turn. Kills the hcom agent tagged with the conversation id
/// (best-effort) and records a note so the panel reflects it. Open like the rest
/// of the chat surface (a local single-user daemon); it only affects the
/// operator's own agent, started by the operator's own message.
pub async fn stop_agent_chat(
    State(state): State<AppState>,
    Path(conversation): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .store
        .get_agent_conversation(&conversation)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Best-effort kill: an already-finished agent is fine (nothing to stop).
    let stopped = lazybones_engine::cancel_agent(&conversation).await.is_ok();
    if !stopped {
        tracing::warn!(conversation = %conversation, "agent stop: hcom kill failed (agent may be gone)");
    }

    // Record a note so the conversation shows it was stopped (and the panel's
    // working indicator clears via this message arriving on the SSE).
    state
        .store
        .append_agent_message(
            &conversation,
            AgentRole::Tool,
            "(stopped by operator)",
            None,
        )
        .await?;

    Ok(Json(serde_json::json!({ "stopped": stopped })))
}

/// `GET /agent/chat/:conversation/stream` — a per-conversation SSE feed of agent
/// messages (`message` events). Only messages for `conversation` are emitted, so
/// agent token streams don't fan out to every connected client like the global
/// `/stream` would (scope §8.4).
pub async fn agent_chat_stream(
    State(state): State<AppState>,
    Path(conversation): Path<String>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let events = BroadcastStream::new(state.store.subscribe())
        .filter_map(move |item| to_sse(item, &conversation));
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

/// Render a bus item as a `message` SSE event iff it belongs to `conversation`.
fn to_sse(
    item: Result<LiveEvent, BroadcastStreamRecvError>,
    conversation: &str,
) -> Option<Result<SseEvent, Infallible>> {
    match item.ok()? {
        LiveEvent::AgentMessage(msg) if msg.conversation_id == conversation => {
            Some(Ok(SseEvent::default()
                .event("message")
                .json_data(msg)
                .ok()?))
        }
        // Ephemeral "what the agent is doing now" tick — a separate `activity`
        // event the panel renders as the live working line (not stored).
        LiveEvent::AgentActivity {
            conversation_id,
            text,
        } if conversation_id == conversation => Some(Ok(SseEvent::default()
            .event("activity")
            .json_data(serde_json::json!({ "text": text }))
            .ok()?)),
        _ => None,
    }
}

/// Project the store's permission profile into the auth crate's capability
/// profile (the API layer owns this seam so the store needs no auth dependency).
fn auth_profile(profile: PermissionProfile) -> ManagementProfile {
    match profile {
        PermissionProfile::ReadOnly => ManagementProfile::ReadOnly,
        PermissionProfile::Author => ManagementProfile::Author,
        PermissionProfile::AuthorAndManage => ManagementProfile::AuthorAndManage,
    }
}
