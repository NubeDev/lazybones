//! `/gh/*` — GitHub operations for the UI, backed by [`lazybones_gh`].
//!
//! Every handler shells out to the user's already-authenticated `gh`/`git`
//! (no token handling here — see the `lazybones-gh` crate). Reads require
//! `Read`; anything that mutates the repo or GitHub (new branch, open/close
//! issue) requires `Author` (loop only), matching the workflow-authoring
//! boundary. `GET /gh/auth` is unguarded — like `/engine`, it's setup state the
//! UI shows before a token exists.
//!
//! Routes take the target repo as a `?dir=` query (reads) or `dir` body field
//! (mutations); `.` means the server's working directory.
//!
//! NB: handlers return *local* DTOs (below), not `lazybones_gh`'s types. The
//! dependency graph carries two axum versions (surrealdb → tonic pulls 0.8
//! alongside our 0.7), which makes axum's `IntoResponse` resolution ambiguous
//! for `Json<T>` where `T` is a foreign serializable type. Owning the wire
//! shape here both sidesteps that and keeps the REST surface decoupled from the
//! crate's internals.

use axum::Json;
use axum::extract::{Path, Query, State};
use lazybones_auth::Capability;
use lazybones_gh::{self as gh, Gh, IssueState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::ApiResult;
use crate::extract::Session;
use crate::state::AppState;

/// `?dir=` — which repo to act on. Defaults to `.`.
#[derive(Debug, Deserialize)]
pub struct DirQuery {
    #[serde(default = "dot")]
    dir: String,
}

fn dot() -> String {
    ".".into()
}

// ---- wire DTOs ----------------------------------------------------------

/// Repo identity for the UI's workspace panel.
#[derive(Debug, Serialize)]
pub struct RepoDto {
    pub full_name: String,
    pub name: String,
    pub owner: String,
    pub url: String,
    pub description: String,
    pub default_branch: Option<String>,
}

impl From<gh::RepoView> for RepoDto {
    fn from(r: gh::RepoView) -> Self {
        Self {
            full_name: r.full_name(),
            default_branch: r.default_branch().map(ToOwned::to_owned),
            name: r.name,
            owner: r.owner.login,
            url: r.url,
            description: r.description,
        }
    }
}

/// One branch in the selector.
#[derive(Debug, Serialize)]
pub struct BranchDto {
    pub name: String,
    pub sha: String,
    pub protected: bool,
}

impl From<gh::Branch> for BranchDto {
    fn from(b: gh::Branch) -> Self {
        Self {
            name: b.name,
            sha: b.sha,
            protected: b.protected,
        }
    }
}

/// One issue.
#[derive(Debug, Serialize)]
pub struct IssueDto {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub url: String,
    pub body: String,
    pub author: Option<String>,
    pub labels: Vec<String>,
}

impl From<gh::Issue> for IssueDto {
    fn from(i: gh::Issue) -> Self {
        Self {
            number: i.number,
            title: i.title,
            state: i.state,
            url: i.url,
            body: i.body,
            author: i.author.map(|a| a.login),
            labels: i.labels.into_iter().map(|l| l.name).collect(),
        }
    }
}

// ---- auth ---------------------------------------------------------------

/// Whether `gh` is installed and logged in. Reported as data, never an error,
/// so the UI can prompt "run `gh auth login`" instead of failing a request.
#[derive(Debug, Serialize)]
pub struct GhAuth {
    /// `gh auth status` succeeded — a usable login exists.
    pub authenticated: bool,
    /// Short reason when not authenticated (the CLI's own message).
    pub detail: Option<String>,
}

/// `GET /gh/auth` — unguarded auth probe for the UI.
pub async fn gh_auth() -> Json<GhAuth> {
    match Gh::new().ensure_auth().await {
        Ok(()) => Json(GhAuth {
            authenticated: true,
            detail: None,
        }),
        Err(e) => Json(GhAuth {
            authenticated: false,
            detail: Some(e.to_string()),
        }),
    }
}

// ---- repo / branches ----------------------------------------------------

/// `GET /gh/repo?dir=` — repo identity + default branch. Requires `Read`.
pub async fn gh_repo(
    State(_): State<AppState>,
    session: Session,
    Query(q): Query<DirQuery>,
) -> ApiResult<Json<RepoDto>> {
    session.require(Capability::Read, "gh:repo", &q.dir)?;
    Ok(Json(Gh::new().repo_view(&q.dir).await?.into()))
}

/// `GET /gh/branches?dir=` — list branches to pick from. Requires `Read`.
pub async fn gh_branches(
    State(_): State<AppState>,
    session: Session,
    Query(q): Query<DirQuery>,
) -> ApiResult<Json<Vec<BranchDto>>> {
    session.require(Capability::Read, "gh:branches", &q.dir)?;
    let branches = Gh::new().branches(&q.dir).await?;
    Ok(Json(branches.into_iter().map(Into::into).collect()))
}

/// Body for creating a branch.
#[derive(Debug, Deserialize)]
pub struct CreateBranchBody {
    #[serde(default = "dot")]
    pub dir: String,
    /// New branch name.
    pub name: String,
    /// Start point (branch/sha); `None` ⇒ current `HEAD`.
    #[serde(default)]
    pub from: Option<String>,
}

/// What we report after making (and checking out) a branch.
#[derive(Debug, Serialize)]
pub struct BranchCreated {
    pub branch: String,
}

/// `POST /gh/branches` — make + check out a new branch. Requires `Author`.
pub async fn gh_create_branch(
    State(_): State<AppState>,
    session: Session,
    Json(body): Json<CreateBranchBody>,
) -> ApiResult<Json<BranchCreated>> {
    session.require(Capability::Author, "gh:branch:create", &body.name)?;
    let gh = Gh::new();
    gh.create_branch(&body.dir, &body.name, body.from.as_deref())
        .await?;
    Ok(Json(BranchCreated {
        branch: gh.current_branch(&body.dir).await?,
    }))
}

/// Body for switching branches.
#[derive(Debug, Deserialize)]
pub struct CheckoutBody {
    #[serde(default = "dot")]
    pub dir: String,
    /// Existing branch to switch to.
    pub branch: String,
}

/// `POST /gh/checkout` — switch to an existing branch. Requires `Author`.
pub async fn gh_checkout(
    State(_): State<AppState>,
    session: Session,
    Json(body): Json<CheckoutBody>,
) -> ApiResult<Json<BranchCreated>> {
    session.require(Capability::Author, "gh:checkout", &body.branch)?;
    let gh = Gh::new();
    gh.checkout(&body.dir, &body.branch).await?;
    Ok(Json(BranchCreated {
        branch: gh.current_branch(&body.dir).await?,
    }))
}

/// `?dir=&force=` — branch-delete options.
#[derive(Debug, Deserialize)]
pub struct DeleteBranchQuery {
    #[serde(default = "dot")]
    dir: String,
    #[serde(default)]
    force: bool,
}

/// `DELETE /gh/branches/:name?dir=&force=` — delete a local branch. Requires
/// `Author`. `force=true` maps to `git branch -D` (drops unmerged work).
pub async fn gh_delete_branch(
    State(_): State<AppState>,
    session: Session,
    Path(name): Path<String>,
    Query(q): Query<DeleteBranchQuery>,
) -> ApiResult<Json<Value>> {
    session.require(Capability::Author, "gh:branch:delete", &name)?;
    Gh::new().delete_branch(&q.dir, &name, q.force).await?;
    Ok(Json(json!({ "deleted": name })))
}

// ---- worktrees ----------------------------------------------------------

/// One worktree row for the UI.
#[derive(Debug, Serialize)]
pub struct WorktreeDto {
    pub path: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub is_main: bool,
    pub locked: bool,
}

impl From<gh::Worktree> for WorktreeDto {
    fn from(w: gh::Worktree) -> Self {
        Self {
            path: w.path,
            branch: w.branch,
            head: w.head,
            is_main: w.is_main,
            locked: w.locked,
        }
    }
}

/// `GET /gh/worktrees?dir=` — list the repo's worktrees. Requires `Read`.
pub async fn gh_worktrees(
    State(_): State<AppState>,
    session: Session,
    Query(q): Query<DirQuery>,
) -> ApiResult<Json<Vec<WorktreeDto>>> {
    session.require(Capability::Read, "gh:worktrees", &q.dir)?;
    let trees = Gh::new().worktrees(&q.dir).await?;
    Ok(Json(trees.into_iter().map(Into::into).collect()))
}

/// Body for removing a worktree.
#[derive(Debug, Deserialize)]
pub struct RemoveWorktreeBody {
    #[serde(default = "dot")]
    pub dir: String,
    /// Worktree working-dir path to remove.
    pub path: String,
    #[serde(default)]
    pub force: bool,
}

/// `DELETE /gh/worktrees` — remove a worktree. Requires `Block` (it tears down
/// a checkout the scheduler may own). The main worktree can never be removed.
pub async fn gh_remove_worktree(
    State(_): State<AppState>,
    session: Session,
    Json(body): Json<RemoveWorktreeBody>,
) -> ApiResult<Json<Value>> {
    session.require(Capability::Block, "gh:worktree:remove", &body.path)?;
    let gh = Gh::new();
    // Defensive: never let the primary checkout be removed from here.
    if let Some(main) = gh.worktrees(&body.dir).await?.into_iter().find(|w| w.is_main)
        && main.path == body.path
    {
        return Err(crate::error::ApiError::bad_request(
            "refusing to remove the main worktree",
        ));
    }
    gh.remove_worktree(&body.dir, &body.path, body.force).await?;
    Ok(Json(json!({ "removed": body.path })))
}

/// Body for pruning worktrees.
#[derive(Debug, Deserialize)]
pub struct PruneWorktreeBody {
    #[serde(default = "dot")]
    pub dir: String,
}

/// `POST /gh/worktrees/prune` — drop stale worktree entries. Requires `Block`.
pub async fn gh_prune_worktrees(
    State(_): State<AppState>,
    session: Session,
    Json(body): Json<PruneWorktreeBody>,
) -> ApiResult<Json<Value>> {
    session.require(Capability::Block, "gh:worktree:prune", &body.dir)?;
    Gh::new().prune_worktrees(&body.dir).await?;
    Ok(Json(json!({ "pruned": true })))
}

// ---- issues -------------------------------------------------------------

/// `?dir=&state=` — list filter for issues.
#[derive(Debug, Deserialize)]
pub struct IssueListQuery {
    #[serde(default = "dot")]
    dir: String,
    /// `open` (default), `closed`, or `all`.
    #[serde(default)]
    state: Option<String>,
}

/// `GET /gh/issues?dir=&state=` — list issues. Requires `Read`.
pub async fn gh_issues(
    State(_): State<AppState>,
    session: Session,
    Query(q): Query<IssueListQuery>,
) -> ApiResult<Json<Vec<IssueDto>>> {
    session.require(Capability::Read, "gh:issues", &q.dir)?;
    let state = match q.state.as_deref() {
        Some("closed") => IssueState::Closed,
        Some("all") => IssueState::All,
        _ => IssueState::Open,
    };
    let issues = Gh::new().issues(&q.dir, state).await?;
    Ok(Json(issues.into_iter().map(Into::into).collect()))
}

/// `GET /gh/issues/:number?dir=` — view one issue. Requires `Read`.
pub async fn gh_issue_view(
    State(_): State<AppState>,
    session: Session,
    Path(number): Path<u64>,
    Query(q): Query<DirQuery>,
) -> ApiResult<Json<IssueDto>> {
    session.require(Capability::Read, "gh:issue:view", &q.dir)?;
    Ok(Json(Gh::new().issue_view(&q.dir, number).await?.into()))
}

/// Body for opening an issue.
#[derive(Debug, Deserialize)]
pub struct CreateIssueBody {
    #[serde(default = "dot")]
    pub dir: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
}

/// What we report after opening an issue.
#[derive(Debug, Serialize)]
pub struct IssueCreated {
    /// URL of the new issue (as `gh issue create` prints).
    pub url: String,
}

/// `POST /gh/issues` — open a new issue. Requires `Author`.
pub async fn gh_create_issue(
    State(_): State<AppState>,
    session: Session,
    Json(body): Json<CreateIssueBody>,
) -> ApiResult<Json<IssueCreated>> {
    session.require(Capability::Author, "gh:issue:create", &body.title)?;
    let url = Gh::new()
        .issue_create(&body.dir, &body.title, &body.body)
        .await?;
    Ok(Json(IssueCreated { url }))
}

/// `POST /gh/issues/:number/close?dir=` — close an issue. Requires `Author`.
pub async fn gh_close_issue(
    State(_): State<AppState>,
    session: Session,
    Path(number): Path<u64>,
    Query(q): Query<DirQuery>,
) -> ApiResult<Json<IssueDto>> {
    session.require(Capability::Author, "gh:issue:close", &q.dir)?;
    let gh = Gh::new();
    gh.issue_close(&q.dir, number).await?;
    Ok(Json(gh.issue_view(&q.dir, number).await?.into()))
}
