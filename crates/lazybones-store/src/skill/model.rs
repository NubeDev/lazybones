//! The durable `Skill` document — a reusable, named block of agent instructions.
//!
//! A skill is a stateless, install-wide recipe of *guidance* (the `body`): the
//! text/instructions an agent should follow for a class of work (e.g. how to do a
//! Rust code review). Like a [`Template`](crate::Template) it has no lifecycle, no
//! run, no claim state — it is authored once and reused. Skills are attached to
//! other entities (templates today) via the generic
//! [`attachment`](crate::attachment) seam; consuming an attached skill in the
//! agent prompt is a deferred concern, not part of this model.

use serde::{Deserialize, Serialize};

/// A reusable block of agent instructions, unique install-wide by `id`
/// (e.g. `code-review-rust`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    /// Friendly, unique id (e.g. `code-review-rust`, `write-tests`).
    pub id: String,
    /// Human title.
    pub title: String,
    /// Optional longer description shown in the picker.
    #[serde(default)]
    pub description: String,
    /// The skill text/instructions an agent follows (markdown).
    #[serde(default)]
    pub body: String,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Skill {
    /// A freshly authored skill stamped `created_at == updated_at == now`.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        body: impl Into<String>,
        now: impl Into<String>,
    ) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            body: body.into(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
