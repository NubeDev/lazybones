use serde::Deserialize;

/// `gh repo view --json ...` result.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoView {
    pub name: String,
    pub owner: Owner,
    pub url: String,
    #[serde(default)]
    pub description: String,
    /// Default branch, e.g. `master` / `main`.
    #[serde(default, rename = "defaultBranchRef")]
    pub default_branch_ref: Option<Ref>,
}

impl RepoView {
    /// Convenience: `owner/name`.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner.login, self.name)
    }

    /// Default branch name, if known.
    pub fn default_branch(&self) -> Option<&str> {
        self.default_branch_ref.as_ref().map(|r| r.name.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Owner {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ref {
    pub name: String,
}

/// One branch, as returned by the `repos/{owner}/{repo}/branches` API.
#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    pub name: String,
    /// Tip commit SHA.
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub protected: bool,
}

/// One entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// Absolute path of the worktree's working directory.
    pub path: String,
    /// Branch checked out, short form (`feat/x`); `None` if detached.
    pub branch: Option<String>,
    /// Tip commit SHA the worktree points at; `None` for an unborn/bare entry.
    pub head: Option<String>,
    /// The main working tree (the repo's primary checkout).
    pub is_main: bool,
    /// `true` if the worktree is locked (`git worktree lock`).
    pub locked: bool,
}

impl Worktree {
    /// Parse the `--porcelain` output of `git worktree list`. Entries are
    /// newline-separated records (`key value` lines) split by a blank line. The
    /// first record is always the main working tree.
    pub fn parse_list(out: &str) -> Vec<Worktree> {
        let mut trees = Vec::new();
        let mut first = true;
        for block in out.split("\n\n") {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }
            let mut path = None;
            let mut branch = None;
            let mut head = None;
            let mut locked = false;
            for line in block.lines() {
                if let Some(p) = line.strip_prefix("worktree ") {
                    path = Some(p.trim().to_string());
                } else if let Some(h) = line.strip_prefix("HEAD ") {
                    head = Some(h.trim().to_string());
                } else if let Some(b) = line.strip_prefix("branch ") {
                    // `refs/heads/feat/x` → `feat/x`.
                    branch = Some(
                        b.trim()
                            .strip_prefix("refs/heads/")
                            .unwrap_or(b.trim())
                            .to_string(),
                    );
                } else if line.trim() == "locked" || line.starts_with("locked ") {
                    locked = true;
                }
            }
            if let Some(path) = path {
                trees.push(Worktree {
                    path,
                    branch,
                    head,
                    is_main: first,
                    locked,
                });
                first = false;
            }
        }
        trees
    }
}
