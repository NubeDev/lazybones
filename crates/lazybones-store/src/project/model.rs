//! The durable `Project` document — the only new noun in the team graph.
//!
//! A project is an ownership/authz anchor (projects.md "Project = ownership root"),
//! not a technical target: it sits `under` a team and contains workflows. It is
//! cloud-authored and single-writer (decisions §3), so its id stays plain
//! (`project:apollo`), never `{org}/{edge}`-namespaced.
//!
//! A project targets **many** repos (decisions §1 Q1) — carried as project config
//! ([`repos`](Project::repos)) and/or `repo:*` tags, distinct from the existing
//! repo/worktree machinery. No single `repo` field is pinned. The owning team is
//! denormalized into [`team`](Project::team) alongside the authoritative
//! `project ->under-> team` edge, so "projects in my team" is one indexed read.

use serde::{Deserialize, Serialize};

/// Whether a project is live or shelved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    /// In flight — the default for a freshly created project.
    #[default]
    Active,
    /// Shelved — kept for history, no new work.
    Archived,
}

impl ProjectStatus {
    /// The lowercase wire/stored form.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectStatus::Active => "active",
            ProjectStatus::Archived => "archived",
        }
    }

    /// Parse the stored form, defaulting an unknown/missing value to `Active`.
    #[must_use]
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("archived") => ProjectStatus::Archived,
            _ => ProjectStatus::Active,
        }
    }
}

/// A project — ownership root for a team's workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Friendly, unique id (e.g. `apollo`).
    pub id: String,
    /// Human title.
    pub title: String,
    /// Lifecycle status (`active` | `archived`).
    #[serde(default)]
    pub status: ProjectStatus,
    /// Denormalized owning-team id (mirrors `project ->under-> team`); `None` until
    /// the project is placed under a team.
    #[serde(default)]
    pub team: Option<String>,
    /// The repo target(s) this project's work spans — config form of the `repo:*`
    /// tags (decisions §1 Q1). Empty when none are pinned.
    #[serde(default)]
    pub repos: Vec<String>,
    /// RFC3339 creation timestamp.
    pub created_at: String,
    /// RFC3339 last-update timestamp.
    pub updated_at: String,
}

impl Project {
    /// A freshly authored, `active` project stamped `created_at == updated_at == now`.
    #[must_use]
    pub fn new(id: impl Into<String>, title: impl Into<String>, now: impl Into<String>) -> Self {
        let now = now.into();
        Self {
            id: id.into(),
            title: title.into(),
            status: ProjectStatus::Active,
            team: None,
            repos: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Set the denormalized owning team (builder style). The authoritative edge is
    /// still written via [`place_project_under_team`](super::place_project_under_team).
    #[must_use]
    pub fn with_team(mut self, team: impl Into<String>) -> Self {
        self.team = Some(team.into());
        self
    }

    /// Set the repo targets carried as config (builder style).
    #[must_use]
    pub fn with_repos(mut self, repos: Vec<String>) -> Self {
        self.repos = repos;
        self
    }
}
