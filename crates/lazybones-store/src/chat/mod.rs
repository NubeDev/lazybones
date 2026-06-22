//! Per-task chat: the durable, two-sided conversation between the operator and a
//! task's agent.
//!
//! The operator posts on the task's hcom thread to steer a running task or to
//! workshop a blocked one back to life; the agent replies on the same thread.
//! This module stores that conversation as append-only rows so a "chat with the
//! agent" view has a single, restart-durable source — independent of hcom's own
//! transcript retention.

pub(crate) mod append;
mod history;
mod model;
mod row;

pub use append::append_chat;
pub use history::chat_history;
pub use model::{ChatMessage, ChatRole};
