//! `GET /tasks/:id/chat` + `POST /tasks/:id/chat` — the per-task conversation.
//!
//! The user story: chat with a task's agent. While the task is **running**, a
//! posted message is delivered live to the agent on its hcom thread to steer it.
//! Once a task is **blocked**, posting a message *revives* it — the next tick
//! re-spawns an agent in the kept worktree with the conversation folded into its
//! prompt (`scheduler::prompt`), so the operator can workshop a failure back to a
//! green finish. Either way the message is written to the durable `chat` store
//! first, so the conversation survives a restart and is the single source the UI
//! renders; the agent's own replies are mirrored in by the hcom tail.

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_store::{ChatMessage, ChatRole, Status, StoreError, Task, Transition};
use serde::Serialize;

use crate::dto::ChatBody;
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

/// `GET /tasks/:id/chat` — the task's conversation, oldest first.
///
/// Open like the hcom log / transcript reads: it only replays history a local
/// operator can already see. `404` if the task is unknown.
pub async fn get_chat(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<ChatMessage>>> {
    // Surface a clear 404 for an unknown task rather than an empty conversation.
    state
        .store
        .get_task(&id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.clone()))?;
    Ok(Json(state.store.chat_history(&id).await?))
}

/// The result of posting a message: the stored message plus how it was delivered.
#[derive(Debug, Serialize)]
pub struct ChatPosted {
    /// The persisted operator message.
    pub message: ChatMessage,
    /// What the post did, given the task's state:
    /// - `delivered` — sent live to the running agent's thread;
    /// - `revived` — the blocked task was revived; the next tick re-spawns it;
    /// - `stored` — recorded as guidance; folded into the prompt at the next claim
    ///   (the task hasn't been claimed yet, or live delivery to hcom failed).
    pub delivery: &'static str,
}

/// `POST /tasks/:id/chat` — post a message to the task's agent.
///
/// Requires `Block` (the operator task-control capability cancel also uses):
/// posting can revive a blocked task, the inverse of cancelling one. The message
/// is stored durably, then acted on by task state:
/// - `running`/`gating`: delivered live on the hcom thread;
/// - `blocked`: the task is revived (`blocked -> ready`) so the loop re-spawns it;
/// - `pending`/`ready`: stored as guidance, folded into the prompt at first claim;
/// - `done`: rejected — restart the task to re-run it.
pub async fn post_chat(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<ChatBody>,
) -> ApiResult<Json<ChatPosted>> {
    session.require(Capability::Block, "chat", &id)?;

    let text = body.text.trim();
    if text.is_empty() {
        return Err(ApiError::bad_request("chat message text must not be empty"));
    }

    let task = state
        .store
        .get_task(&id)
        .await?
        .ok_or_else(|| StoreError::TaskNotFound(id.clone()))?;

    // A stopped (paused) workflow's tasks are not revivable: a chat post revives a
    // blocked task (and even on a pending one is folded into the next claim), so
    // it would steer/re-run work in a run the operator paused. Refuse until the
    // workflow is resumed — consistent with retry/auto-retry.
    super::guard::ensure_run_revivable(&state, &task).await?;

    // A done task is terminal and merged; there is no agent and no kept worktree
    // to resume. Restarting is the right tool, so reject before storing a message
    // that could never be acted on.
    if task.status == Status::Done {
        return Err(ApiError::conflict(format!(
            "task `{id}` is done; restart it to re-run rather than chatting"
        )));
    }

    // Durable first: the conversation is the source of truth regardless of whether
    // live delivery or the revive spawn succeeds. Group under the workflow id so
    // it parallels the hcom log's keying (falls back to the run label).
    let run = task.run_id.clone().unwrap_or_else(|| task.run.clone());
    let message = state
        .store
        .append_chat(&run, &id, ChatRole::User, text, None)
        .await?;

    let delivery = deliver(&state, &session, &task, text).await?;
    Ok(Json(ChatPosted { message, delivery }))
}

/// Act on a stored message per the task's state, returning the delivery label.
async fn deliver(
    state: &AppState,
    session: &Session,
    task: &Task,
    text: &str,
) -> ApiResult<&'static str> {
    match task.status {
        // Live steer: the agent is up and listening on its thread.
        Status::Running | Status::Gating => {
            match lazybones_engine::send_to_agent(&task.id, text).await {
                Ok(()) => Ok("delivered"),
                // A send failure (agent already gone, hcom hiccup) must not lose
                // the message — it is stored and will reach the agent if it is
                // reclaimed and re-spawned. Surface "stored", don't fail the post.
                Err(e) => {
                    tracing::warn!(task = %task.id, "chat: live send failed (message stored): {e}");
                    Ok("stored")
                }
            }
        }
        // Revive: re-open the blocked task so the loop re-spawns it in its kept
        // worktree with this conversation in the prompt.
        Status::Blocked => {
            state
                .store
                .transition(&task.id, Transition::Revive, session.actor())
                .await?;
            Ok("revived")
        }
        // Not yet claimed: the message is guidance the first spawn will fold in.
        Status::Pending | Status::Ready => Ok("stored"),
        // Rejected earlier in `post_chat`.
        Status::Done => unreachable!("done is rejected before delivery"),
    }
}
