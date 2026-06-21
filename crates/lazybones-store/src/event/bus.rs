//! In-process live event bus — a broadcast of everything the UI shows live.
//!
//! Carries two kinds of signal (SCOPE.md: "live queries so status is a push
//! feed, not a poll"):
//!
//! - [`LiveEvent::Transition`] — a durable lifecycle change, also recorded in the
//!   run log. [`StoreHandle::transition`](crate::StoreHandle) publishes it after
//!   the `event` row is persisted.
//! - [`LiveEvent::Activity`] — an ephemeral agent progress message ("running
//!   cargo test…"), published by heartbeats/activity reports and never persisted.
//!
//! The REST `/stream` SSE route subscribes and emits one named SSE event per
//! variant, so a dashboard sees both status changes and live agent chatter.

use tokio::sync::broadcast;

use crate::chat::ChatMessage;
use crate::hcom_log::HcomLogEntry;

use crate::agent_chat::AgentMessage;

use super::activity::Activity;
use super::row::Event;

/// How many unconsumed events the bus buffers per subscriber before the oldest
/// are dropped. A lagging client gets a `Lagged` signal and recovers by
/// refetching; this is generous for the message rate a handful of agents make.
const BUS_CAPACITY: usize = 256;

/// One item on the live feed: a lifecycle transition or an agent activity note.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveEvent {
    /// A durable status change (also in the run log).
    Transition(Event),
    /// An ephemeral agent progress message (not persisted).
    Activity(Activity),
    /// A raw hcom event the tail just ingested — also durable in the `hcom_log`
    /// table, published here for the live edge (docs/hcom-logs-scope.md). Because
    /// persistence happens before publish, anything streamed is already
    /// re-fetchable via `GET /runs/:id/hcom`.
    HcomLog(HcomLogEntry),
    /// A chat message just appended to a task's conversation — also durable in
    /// the `chat` table, published here for the live edge so a "chat with the
    /// agent" view updates without polling. Carries both operator messages and
    /// mirrored agent replies.
    Chat(ChatMessage),
    /// A management-agent message just appended to a conversation — also durable
    /// in the `agent_message` table, published here for the live edge so the
    /// global agent chat panel updates without polling. Carries operator turns,
    /// mirrored agent replies, and tool-action transparency notes
    /// (`docs/agent/lazybones-agent-scope.md` §8.4).
    AgentMessage(AgentMessage),
    /// An ephemeral management-agent *activity* tick (NOT persisted) — what the
    /// agent is doing right now ("Running Bash…", "Reading…"), derived from its
    /// hcom tool-status events. Published on the per-conversation SSE so the panel
    /// shows live progress instead of a blank spinner; history stays clean.
    AgentActivity {
        /// The conversation this activity belongs to.
        conversation_id: String,
        /// A short human-readable note ("Running Bash…").
        text: String,
    },
}

/// A cloneable publish/subscribe handle for the live feed.
///
/// Backed by a [`broadcast`] channel: every published [`LiveEvent`] reaches
/// every current subscriber. Cloning shares the same underlying channel.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<LiveEvent>,
}

impl EventBus {
    /// Create a bus with the default buffer capacity.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(BUS_CAPACITY);
        Self { sender }
    }

    /// Publish an event to every subscriber. A send with no subscribers is not
    /// an error — durable transitions live in the `event` table regardless, and
    /// activities are best-effort by design.
    pub fn publish(&self, event: LiveEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to the live feed. The receiver sees events published after it
    /// subscribes; backlog beyond [`BUS_CAPACITY`] is dropped with `Lagged`.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<LiveEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
