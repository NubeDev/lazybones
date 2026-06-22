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

/// One parameter a structured skill action accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillParam {
    /// The parameter name (referenced as `{name}` in the path/body templates).
    pub name: String,
    /// Whether the agent must supply it (missing required params block the call).
    #[serde(default)]
    pub required: bool,
    /// A human description guiding what to put here.
    #[serde(default)]
    pub description: String,
}

/// An optional, typed action a skill exposes for *deterministic* execution — the
/// structured counterpart to a pure-markdown runbook
/// (`docs/agent/lazybones-agent-scope.md` §6.1, open question 2). It describes a
/// parameterised REST call: the agent gathers the named params and the call is
/// made by substituting them into the templates. Skills without an action remain
/// plain advisory markdown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillAction {
    /// The HTTP method, e.g. `"POST"`.
    pub method: String,
    /// The REST path template, with `{param}` placeholders, e.g.
    /// `"/workflows/{id}/tasks"`.
    pub path_template: String,
    /// An optional JSON body template; `{param}` placeholders in string values
    /// are substituted before the call. `None` for a body-less call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_template: Option<serde_json::Value>,
    /// The parameters the action accepts.
    #[serde(default)]
    pub params: Vec<SkillParam>,
}

impl SkillAction {
    /// Validate the action's internal consistency: every `{name}` placeholder in
    /// the path (and string body values) must be a declared param, and the
    /// method must be a mutating verb. Returns a human-readable error otherwise.
    ///
    /// # Errors
    /// Returns a message describing the first inconsistency found.
    pub fn validate(&self) -> Result<(), String> {
        const METHODS: [&str; 3] = ["POST", "PUT", "DELETE"];
        if !METHODS.contains(&self.method.as_str()) {
            return Err(format!(
                "action method `{}` must be one of POST/PUT/DELETE",
                self.method
            ));
        }
        let declared: std::collections::HashSet<&str> =
            self.params.iter().map(|p| p.name.as_str()).collect();
        let mut text = self.path_template.clone();
        if let Some(body) = &self.body_template {
            text.push(' ');
            text.push_str(&body.to_string());
        }
        for name in placeholders(&text) {
            if !declared.contains(name.as_str()) {
                return Err(format!("template references undeclared param `{{{name}}}`"));
            }
        }
        Ok(())
    }
}

/// Extract `{name}` placeholder names from a template string.
fn placeholders(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        if let Some(close) = rest[open + 1..].find('}') {
            let name = &rest[open + 1..open + 1 + close];
            if !name.is_empty() && !name.contains('{') {
                out.push(name.to_owned());
            }
            rest = &rest[open + 1 + close + 1..];
        } else {
            break;
        }
    }
    out
}

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
    /// An optional typed action for deterministic execution; `None` for a plain
    /// markdown-runbook skill (the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<SkillAction>,
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
            action: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Attach a structured action to this skill (builder style).
    #[must_use]
    pub fn with_action(mut self, action: SkillAction) -> Self {
        self.action = Some(action);
        self
    }
}
