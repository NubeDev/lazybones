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
