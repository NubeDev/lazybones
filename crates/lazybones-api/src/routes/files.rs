//! `/files/*` — read-only repo file browser + diff for the UI's "Files" tab,
//! backed by [`lazybones_gh`].
//!
//! Like `/gh/*`, every handler shells out to the user's `git` (no token here)
//! and takes the target repo/worktree as a `?dir=` query. All three are reads,
//! requiring `Read`. A repo-relative `?rel=` selects a subdirectory (tree) or a
//! file (read/diff); we reject any `rel` that escapes the repo (`..`, absolute)
//! so the browser can't be steered outside the checkout.
//!
//! NB: returns local DTOs, not `lazybones_gh`'s types — same axum-version
//! reason as `gh.rs`.

use axum::Json;
use axum::extract::{Query, State};
use lazybones_auth::Capability;
use lazybones_gh::{self as gh, Gh};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::state::AppState;

fn dot() -> String {
    ".".into()
}

/// Reject a repo-relative path that could escape the repo: absolute paths or any
/// `..` component. Empty (`""`) is the repo root and is fine.
fn safe_rel(rel: &str) -> ApiResult<&str> {
    let rel = rel.trim_matches('/');
    if rel.starts_with('/')
        || rel.split('/').any(|seg| seg == "..")
        || rel.contains('\0')
    {
        return Err(ApiError::bad_request(format!("illegal path: {rel}")));
    }
    Ok(rel)
}

// ---- tree listing -------------------------------------------------------

/// `?dir=&base=` — which repo's tree to list, and an optional base branch whose
/// branch-vs-base changes also decorate the tree.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "dot")]
    dir: String,
    /// Base branch for branch-vs-base decoration; omitted ⇒ uncommitted only.
    #[serde(default)]
    base: Option<String>,
}

/// One file-tree entry for the UI. `status` is the single-letter git change
/// tag (`M`/`A`/`D`/`U`) for *files*; `null` when unchanged. Directory rollup
/// (a folder is "changed" if any descendant is) is done client-side.
#[derive(Debug, Serialize)]
pub struct TreeEntryDto {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub status: Option<char>,
}

/// `GET /files/tree?dir=&base=` — the *whole* repo file tree (VSCode-style),
/// each file tagged with its git status. Requires `Read`.
pub async fn list_tree(
    State(_): State<AppState>,
    session: Session,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<TreeEntryDto>>> {
    session.require(Capability::Read, "files:tree", &q.dir)?;
    let base = q.base.as_deref().filter(|b| !b.is_empty());
    let (entries, status) = Gh::new().tree(&q.dir, base).await?;
    let dto = entries
        .into_iter()
        .map(|e| {
            // Resolve against exact matches *and* collapsed untracked dirs (so
            // files inside an untracked `foo/` are tagged). Dirs get a tag only
            // when they're themselves under an untracked dir; tracked-change
            // roll-up onto ancestor dirs is done client-side from the children.
            let tag = gh::resolve_status(&status, &e.path).map(|k| k.letter());
            TreeEntryDto {
                name: e.name,
                path: e.path,
                is_dir: e.is_dir,
                status: tag,
            }
        })
        .collect();
    Ok(Json(dto))
}

// ---- file read ----------------------------------------------------------

/// `?dir=&rel=` — which repo and which file to read.
#[derive(Debug, Deserialize)]
pub struct ReadQuery {
    #[serde(default = "dot")]
    dir: String,
    /// Repo-relative file path (required).
    rel: String,
}

/// A file's contents for the viewer. `binary` flags content that isn't valid
/// text, so the UI can show "binary file" instead of mojibake.
#[derive(Debug, Serialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub binary: bool,
}

/// `GET /files/read?dir=&rel=` — read one file from the working tree. Requires
/// `Read`.
pub async fn read_file(
    State(_): State<AppState>,
    session: Session,
    Query(q): Query<ReadQuery>,
) -> ApiResult<Json<FileContent>> {
    session.require(Capability::Read, "files:read", &q.dir)?;
    let rel = safe_rel(&q.rel)?;
    if rel.is_empty() {
        return Err(ApiError::bad_request("no file given"));
    }
    let content = Gh::new().read_file(&q.dir, rel).await?;
    // A NUL byte (or the lossy replacement char) is a cheap binary heuristic;
    // matches how editors decide "this isn't text".
    let binary = content.contains('\0') || content.contains('\u{FFFD}');
    Ok(Json(FileContent {
        path: rel.to_string(),
        content,
        binary,
    }))
}

// ---- diff ---------------------------------------------------------------

/// `?dir=&base=&rel=` — diff options.
#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    #[serde(default = "dot")]
    dir: String,
    /// Base branch to diff the current branch against (merge-base, `base...`).
    /// Omitted ⇒ uncommitted working changes (`git diff HEAD`).
    #[serde(default)]
    base: Option<String>,
    /// Repo-relative path to scope the diff to; omitted ⇒ whole repo.
    #[serde(default)]
    rel: Option<String>,
}

/// A unified diff plus the parameters it was computed for.
#[derive(Debug, Serialize)]
pub struct DiffResult {
    /// `git diff` unified output (empty string = no changes).
    pub diff: String,
    /// The base used, if any (echoed so the UI can label the view).
    pub base: Option<String>,
}

/// `GET /files/diff?dir=&base=&rel=` — unified diff of the working tree.
/// `base` set ⇒ branch-vs-base; unset ⇒ uncommitted changes. Requires `Read`.
pub async fn diff(
    State(_): State<AppState>,
    session: Session,
    Query(q): Query<DiffQuery>,
) -> ApiResult<Json<DiffResult>> {
    session.require(Capability::Read, "files:diff", &q.dir)?;
    let base = q.base.as_deref().filter(|b| !b.is_empty());
    let rel = match q.rel.as_deref().filter(|r| !r.is_empty()) {
        Some(r) => Some(safe_rel(r)?),
        None => None,
    };
    let diff = Gh::new().diff(&q.dir, base, rel).await?;
    Ok(Json(DiffResult {
        diff,
        base: base.map(ToOwned::to_owned),
    }))
}
