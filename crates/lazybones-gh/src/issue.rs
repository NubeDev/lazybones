use serde::Deserialize;

/// Which issues to list. Mirrors `gh issue list --state <…>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueState {
    Open,
    Closed,
    All,
}

impl IssueState {
    pub(crate) fn as_arg(self) -> &'static str {
        match self {
            IssueState::Open => "open",
            IssueState::Closed => "closed",
            IssueState::All => "all",
        }
    }
}

/// A GitHub issue, from `gh issue list/view --json ...`.
#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    /// `OPEN` / `CLOSED`.
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct Author {
    #[serde(default)]
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    pub name: String,
}

/// One comment on an issue (or PR), from `gh issue view --json comments`.
#[derive(Debug, Clone, Deserialize)]
pub struct Comment {
    #[serde(default)]
    pub author: Option<Author>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub url: String,
    /// RFC3339 timestamp.
    #[serde(default, rename = "createdAt")]
    pub created_at: Option<String>,
}

/// Wrapper for `gh issue view <n> --json comments`, whose payload is a single
/// object `{ "comments": [...] }`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CommentsView {
    #[serde(default)]
    pub comments: Vec<Comment>,
}
