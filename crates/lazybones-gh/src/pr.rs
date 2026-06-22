use serde::Deserialize;

use crate::issue::{Author, Label};

/// Which pull requests to list. Mirrors `gh pr list --state <…>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Closed,
    Merged,
    All,
}

impl PrState {
    pub(crate) fn as_arg(self) -> &'static str {
        match self {
            PrState::Open => "open",
            PrState::Closed => "closed",
            PrState::Merged => "merged",
            PrState::All => "all",
        }
    }
}

/// How to merge a pull request. Mirrors `gh pr merge`'s mutually-exclusive
/// strategy flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    pub(crate) fn as_flag(self) -> &'static str {
        match self {
            MergeMethod::Merge => "--merge",
            MergeMethod::Squash => "--squash",
            MergeMethod::Rebase => "--rebase",
        }
    }
}

/// A GitHub pull request, from `gh pr list/view --json ...`.
#[derive(Debug, Clone, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    /// `OPEN` / `CLOSED` / `MERGED`.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub author: Option<Author>,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Source branch (the PR's `head`).
    #[serde(default, rename = "headRefName")]
    pub head_ref: String,
    /// Target branch (the PR's `base`).
    #[serde(default, rename = "baseRefName")]
    pub base_ref: String,
    #[serde(default, rename = "isDraft")]
    pub is_draft: bool,
    /// `MERGEABLE` / `CONFLICTING` / `UNKNOWN` (GitHub's own computed value).
    #[serde(default)]
    pub mergeable: String,
    /// RFC3339 timestamps from GitHub. `closed_at`/`merged_at` are null while the
    /// PR is open; a merged PR has both set.
    #[serde(default, rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: Option<String>,
    #[serde(default, rename = "closedAt")]
    pub closed_at: Option<String>,
    #[serde(default, rename = "mergedAt")]
    pub merged_at: Option<String>,
}
