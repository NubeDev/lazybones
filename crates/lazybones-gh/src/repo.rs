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

/// One local branch, from `git for-each-ref` — works with no remote and offline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBranch {
    pub name: String,
    /// Short tip SHA.
    pub sha: String,
    /// Upstream tracking ref (e.g. `origin/master`), if the branch tracks one.
    pub upstream: Option<String>,
    /// Commits ahead of upstream; `0` when no upstream.
    pub ahead: u32,
    /// Commits behind upstream; `0` when no upstream.
    pub behind: u32,
}

impl LocalBranch {
    /// Parse one tab-delimited record emitted by the `--format` we pass to
    /// `git for-each-ref` (see `Gh::branches_local`). Returns `None` for an
    /// empty/garbled line. Fields: `name \t sha \t upstream \t ahead \t behind`.
    pub fn parse_line(line: &str) -> Option<LocalBranch> {
        let mut f = line.split('\t');
        let name = f.next()?.trim();
        if name.is_empty() {
            return None;
        }
        let sha = f.next().unwrap_or("").trim().to_string();
        let upstream = match f.next().unwrap_or("").trim() {
            "" => None,
            u => Some(u.to_string()),
        };
        // `%(upstream:track,nobracket)` field, e.g. "ahead 2, behind 1",
        // "ahead 3", "behind 4", "gone", or "".
        let track = f.next().unwrap_or("").trim();
        let (ahead, behind) = parse_track(track);
        Some(LocalBranch {
            name: name.to_string(),
            sha,
            upstream,
            ahead,
            behind,
        })
    }
}

/// Parse git's `%(upstream:track,nobracket)` output into `(ahead, behind)`.
/// Forms seen: `"ahead 2, behind 1"`, `"ahead 3"`, `"behind 4"`, `"gone"`, `""`.
/// Anything else (no upstream) yields `(0, 0)`.
fn parse_track(s: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    for part in s.split(',') {
        let mut it = part.split_whitespace();
        match (it.next(), it.next()) {
            (Some("ahead"), Some(n)) => ahead = n.parse().unwrap_or(0),
            (Some("behind"), Some(n)) => behind = n.parse().unwrap_or(0),
            _ => {}
        }
    }
    (ahead, behind)
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
