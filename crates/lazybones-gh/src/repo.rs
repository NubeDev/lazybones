use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// One entry in a single level of the repo's file tree: either a subdirectory
/// or a file, relative to the directory being listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Last path component (the display name).
    pub name: String,
    /// Repo-relative path (e.g. `crates/lazybones-gh/src/repo.rs`), usable as
    /// the next `?rel=` step (dirs) or to read/diff a file.
    pub path: String,
    /// `true` for a directory (has children), `false` for a file.
    pub is_dir: bool,
}

impl TreeEntry {
    /// Build one directory level from the NUL-delimited output of
    /// `git ls-files -z` (repo-relative paths). `rel` is the directory to list
    /// (`""` => repo root); only its *immediate* children are returned, with
    /// each intermediate directory collapsed to a single dir entry. Sorted
    /// dirs-first, then by name.
    pub fn from_ls_files(out: &str, rel: &str) -> Vec<TreeEntry> {
        // Normalise the prefix to `""` or `"foo/bar/"` so a simple strip works.
        let prefix = match rel.trim_matches('/') {
            "" => String::new(),
            r => format!("{r}/"),
        };

        let mut dirs: BTreeSet<String> = BTreeSet::new();
        let mut files: BTreeSet<String> = BTreeSet::new();
        for path in out.split('\0').filter(|p| !p.is_empty()) {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue; // not under `rel`
            };
            if rest.is_empty() {
                continue;
            }
            match rest.split_once('/') {
                // A nested path => the first segment is a child directory.
                Some((dir, _)) => {
                    dirs.insert(dir.to_string());
                }
                // No slash => `rest` is a file directly in `rel`.
                None => {
                    files.insert(rest.to_string());
                }
            }
        }

        let mk = |name: String, is_dir: bool| TreeEntry {
            path: format!("{prefix}{name}"),
            name,
            is_dir,
        };
        dirs.into_iter()
            .map(|d| mk(d, true))
            .chain(files.into_iter().map(|f| mk(f, false)))
            .collect()
    }

    /// Build the *whole* repo file tree (every directory and file, not one
    /// level) from NUL-delimited `git ls-files -z`. The frontend renders this
    /// as a single expandable tree (VSCode-style), so it needs every node up
    /// front. Directories are synthesised from the file paths. Sorted so each
    /// directory's children are dirs-first then files, and a dir always sorts
    /// immediately before its own subtree (path order with a `/` tweak).
    pub fn full_tree(out: &str) -> Vec<TreeEntry> {
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        let mut files: BTreeSet<String> = BTreeSet::new();
        for raw in out.split('\0').filter(|p| !p.is_empty()) {
            // `git ls-files` emits a *nested* git checkout (submodule gitlink or
            // a registered worktree) as a path with a trailing slash, e.g.
            // `.lazy/wt/test-be/`. It's an opaque directory, not a file — strip
            // the slash and record it as a leaf dir (with no children) so we
            // don't synthesise an empty-named "file" that can't be read.
            let (path, is_nested) = match raw.strip_suffix('/') {
                Some(p) => (p, true),
                None => (raw, false),
            };
            if is_nested {
                dirs.insert(path.to_string());
            } else {
                files.insert(path.to_string());
            }
            // Register every ancestor directory of this path.
            let mut acc = String::new();
            let mut comps = path.split('/').peekable();
            while let Some(comp) = comps.next() {
                if comps.peek().is_none() {
                    break; // last component is the file/leaf itself
                }
                if !acc.is_empty() {
                    acc.push('/');
                }
                acc.push_str(comp);
                dirs.insert(acc.clone());
            }
        }

        let mut entries: Vec<TreeEntry> = dirs
            .into_iter()
            .map(|p| TreeEntry::from_path(p, true))
            .chain(files.into_iter().map(|p| TreeEntry::from_path(p, false)))
            .collect();
        // Sort by a key that interleaves a dir directly before its children and
        // keeps dirs ahead of sibling files. We compare component-wise: at the
        // point two paths diverge, a directory (has more components after) wins
        // over a file, and otherwise it's lexicographic.
        entries.sort_by_key(tree_order);
        entries
    }

    /// Split a repo-relative `path` into a [`TreeEntry`] with its display name.
    fn from_path(path: String, is_dir: bool) -> TreeEntry {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        TreeEntry { name, path, is_dir }
    }
}

/// Sort key that lays a tree out depth-first, VSCode-style: within every
/// directory, sub-directories come before files, each ordered by name.
///
/// We compare component-by-component. For each component we emit
/// `(is_leaf_file, name)`: a component that the path continues past is an
/// intermediate directory (`is_leaf_file = 0`), so it sorts ahead of any
/// sibling that *ends* there as a file (`is_leaf_file = 1`). Because the
/// directory/file distinction is the first field, dirs always precede sibling
/// files at the same level regardless of name; a directory node and its own
/// subtree share the prefix, so the dir (shorter key) sorts just above them.
fn tree_order(e: &TreeEntry) -> Vec<(u8, String)> {
    let last = e.path.matches('/').count();
    e.path
        .split('/')
        .enumerate()
        .map(|(i, c)| {
            // The final component is a file only when the entry itself is a file.
            let is_leaf_file = if i == last && !e.is_dir { 1 } else { 0 };
            (is_leaf_file, c.to_string())
        })
        .collect()
}

/// How a path changed relative to the comparison point (working tree, or a
/// base branch). Mirrors git's porcelain/`--name-status` letters, collapsed to
/// what a file-tree decoration needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Modified (content changed) — also covers renames' new path.
    Modified,
    /// Added / newly tracked, or untracked-but-not-ignored.
    Added,
    /// Deleted.
    Deleted,
    /// Untracked (present on disk, not in the index).
    Untracked,
}

impl ChangeKind {
    /// The single-letter tag the UI shows (`M`/`A`/`D`/`U`).
    pub fn letter(self) -> char {
        match self {
            ChangeKind::Modified => 'M',
            ChangeKind::Added => 'A',
            ChangeKind::Deleted => 'D',
            ChangeKind::Untracked => 'U',
        }
    }

    /// Parse one porcelain status code (the two `XY` chars) into a kind.
    /// We collapse staged/unstaged into one signal — the tree only needs "this
    /// changed", not the index/worktree split.
    fn from_porcelain(xy: &str) -> Option<ChangeKind> {
        let bytes = xy.as_bytes();
        let x = bytes.first().copied().unwrap_or(b' ');
        let y = bytes.get(1).copied().unwrap_or(b' ');
        if x == b'?' || y == b'?' {
            return Some(ChangeKind::Untracked);
        }
        // Prefer the more meaningful of the two columns.
        for c in [x, y] {
            match c {
                b'A' => return Some(ChangeKind::Added),
                b'D' => return Some(ChangeKind::Deleted),
                b'M' | b'R' | b'C' | b'T' | b'U' => return Some(ChangeKind::Modified),
                _ => {}
            }
        }
        None
    }

    /// Parse one `git diff --name-status` letter (first char of a record).
    fn from_name_status(c: char) -> Option<ChangeKind> {
        match c {
            'A' => Some(ChangeKind::Added),
            'D' => Some(ChangeKind::Deleted),
            'M' | 'R' | 'C' | 'T' => Some(ChangeKind::Modified),
            _ => None,
        }
    }
}

/// Parse `git status --porcelain=v1 -z` into a `path → ChangeKind` map (the
/// uncommitted working-tree changes). NUL-delimited; rename records carry two
/// NUL-separated paths (`R old\0new`) — we key the *new* path.
pub fn parse_status(out: &str) -> BTreeMap<String, ChangeKind> {
    let mut map = BTreeMap::new();
    let mut fields = out.split('\0').filter(|f| !f.is_empty()).peekable();
    while let Some(rec) = fields.next() {
        // Record: `XY <path>` (3-char prefix: two status chars + a space).
        if rec.len() < 3 {
            continue;
        }
        let (xy, path) = rec.split_at(2);
        let path = path.trim_start();
        let kind = ChangeKind::from_porcelain(xy);
        // A rename ('R'/'C' in either column) has the destination path in the
        // *next* NUL field; the path attached here is the old name.
        let renamed = xy.contains('R') || xy.contains('C');
        let key = if renamed {
            fields.next().unwrap_or(path).to_string()
        } else {
            // Keep the trailing slash on an untracked *directory* (git collapses
            // `?? foo/`); `resolve_status` prefix-matches the files under it.
            path.to_string()
        };
        if let Some(kind) = kind {
            map.insert(key, kind);
        }
    }
    map
}

/// Resolve the [`ChangeKind`] for a repo-relative `path` against a status map.
/// Matches the exact path first, then any ancestor recorded as a collapsed
/// untracked directory (`git status` reports `?? foo/`, not each file under it),
/// so every file inside an untracked dir is tagged. Returns `None` if unchanged.
pub fn resolve_status(
    map: &BTreeMap<String, ChangeKind>,
    path: &str,
) -> Option<ChangeKind> {
    if let Some(k) = map.get(path) {
        return Some(*k);
    }
    // The path may itself be a collapsed untracked dir (keyed `path/`).
    if let Some(k) = map.get(&format!("{path}/")) {
        return Some(*k);
    }
    // Walk ancestors: `a/b/c` → check `a/b/`, then `a/`.
    let mut rest = path;
    while let Some(slash) = rest.rfind('/') {
        let dir = &rest[..slash];
        if let Some(k) = map.get(&format!("{dir}/")) {
            return Some(*k);
        }
        rest = dir;
    }
    None
}

/// Parse `git diff --name-status -z base...` into a `path → ChangeKind` map
/// (what the current branch changed vs the base). Records are `STATUS\0path`
/// (rename: `Rxxx\0old\0new`), so we walk fields pairwise.
pub fn parse_name_status(out: &str) -> BTreeMap<String, ChangeKind> {
    let mut map = BTreeMap::new();
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    while let Some(status) = fields.next() {
        let letter = status.chars().next().unwrap_or(' ');
        let kind = ChangeKind::from_name_status(letter);
        let renamed = matches!(letter, 'R' | 'C');
        // Rename/copy: skip the old path, key the new one (the 2nd path field).
        if renamed {
            let _old = fields.next();
            if let (Some(new), Some(kind)) = (fields.next(), kind) {
                map.insert(new.to_string(), kind);
            }
        } else if let (Some(path), Some(kind)) = (fields.next(), kind) {
            map.insert(path.to_string(), kind);
        }
    }
    map
}

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
