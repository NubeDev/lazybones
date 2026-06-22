//! `GET /stream` — a live Server-Sent Events feed of run activity.
//!
//! The push side of the status surface (SCOPE.md: "live queries so status is a
//! push feed, not a poll"). Three SSE event types flow over one connection:
//!
//! - `transition` — a lifecycle status change (the same [`Event`] rows that
//!   `GET /runs/:id` replays, but live).
//! - `activity` — an ephemeral agent progress message ("running cargo test…"),
//!   so the user can see the agent is actually working.
//! - `hcom_log` — a raw hcom event the tail just ingested (also durable; the same
//!   rows `GET /runs/:id/hcom` replays). The live edge of the agent's own
//!   messages/status/lifecycle (docs/hcom-logs-scope.md).
//! - `chat` — a message just appended to a task's conversation (also durable; the
//!   same rows `GET /tasks/:id/chat` replays). Carries operator messages and
//!   mirrored agent replies so a "chat with the agent" view updates live.
//!
//! The browser uses `EventSource`, which reconnects on its own; this stream only
//! carries items that occur while connected, so a client reconciles by refetching
//! the task list on (re)connect.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::Sse;
use axum::response::sse::{Event as SseEvent, KeepAlive};
use lazybones_store::LiveEvent;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{Stream, StreamExt};

use crate::state::AppState;

/// Subscribe to the live feed.
///
/// Each [`LiveEvent`] is emitted as a named SSE message (`transition` or
/// `activity`) whose `data` is the event JSON. A 15s keep-alive comment holds the
/// connection open through idle periods and proxies. A lagging client (it fell
/// behind the bus buffer) silently skips the dropped items rather than erroring —
/// it recovers by refetching, since the durable run log remains complete.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let events = BroadcastStream::new(state.store.subscribe()).filter_map(to_sse);
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

/// Render one bus item as a named SSE message, dropping lagged/unserializable items.
fn to_sse(
    item: Result<LiveEvent, BroadcastStreamRecvError>,
) -> Option<Result<SseEvent, Infallible>> {
    let sse = match item.ok()? {
        LiveEvent::Transition(event) => SseEvent::default().event("transition").json_data(event),
        LiveEvent::Activity(activity) => SseEvent::default().event("activity").json_data(activity),
        LiveEvent::HcomLog(entry) => SseEvent::default().event("hcom_log").json_data(entry),
        LiveEvent::Chat(message) => SseEvent::default().event("chat").json_data(message),
        // Lazybones-Agent messages + activity ticks are carried only on the
        // per-conversation stream (`/agent/chat/:conversation/stream`), never
        // fanned out to every global `/stream` client (scope §8.4).
        LiveEvent::AgentMessage(_) | LiveEvent::AgentActivity { .. } => return None,
    };
    Some(Ok(sse.ok()?))
}
