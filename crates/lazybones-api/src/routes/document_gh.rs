//! `/documents/:id/{repo,gh/*,publish}` — GitHub publishing for a document.
//!
//! Reuses [`lazybones_gh::Gh`] (the user's authenticated `gh`/`git`) to set a
//! repo target on a document, then branch → commit the rendered doc → open a
//! PR/issue. Publishing a document **is a document action**, so every route here
//! is guarded by [`Capability::Document`] (not the task-records `Author`), and the
//! resulting branch/issue/PR links are persisted back onto the document's
//! [`DocRepo`](lazybones_store::DocRepo), mirroring the task's issue linkage.

use std::path::Path as FsPath;

use axum::Json;
use axum::extract::{Path, State};
use lazybones_auth::Capability;
use lazybones_gh::Gh;
use lazybones_store::{DocRepo, Document};
use serde::{Deserialize, Serialize};

use crate::dto::SetDocRepoBody;
use crate::error::{ApiError, ApiResult};
use crate::extract::Session;
use crate::routes::document_render::assemble_markdown;
use crate::routes::documents::require_document;
use crate::state::AppState;

/// `POST /documents/:id/gh/commit` body: the commit message + push toggle.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CommitBody {
    pub message: Option<String>,
    pub push: bool,
}

/// `POST /documents/:id/gh/pr` body: PR title/body/draft overrides.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PrBody {
    pub title: Option<String>,
    pub body: Option<String>,
    pub draft: bool,
}

/// `POST /documents/:id/gh/issue` body: issue title/body overrides.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct IssueBody {
    pub title: Option<String>,
    pub body: Option<String>,
}

/// `POST /documents/:id/publish` body: branch → commit → PR, in one call.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PublishBody {
    pub message: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
}

/// `{ branch }` after creating a branch.
#[derive(Debug, Serialize)]
pub struct BranchOut {
    pub branch: String,
}

/// `{ committed, pushed, output_path }` after a commit.
#[derive(Debug, Serialize)]
pub struct CommitOut {
    pub committed: bool,
    pub pushed: bool,
    pub output_path: String,
}

/// `{ url }` after opening a PR/issue.
#[derive(Debug, Serialize)]
pub struct UrlOut {
    pub url: String,
}

/// `{ branch, pr_url }` after a one-call publish.
#[derive(Debug, Serialize)]
pub struct PublishOut {
    pub branch: String,
    pub pr_url: String,
}

/// The document's repo target, or `400` if none has been set.
fn repo_of(doc: &Document) -> ApiResult<DocRepo> {
    doc.repo
        .clone()
        .ok_or_else(|| ApiError::bad_request("document has no repo target; PUT /documents/:id/repo first"))
}

/// The branch name for a document: `{branch_prefix|"doc/"}{id}`.
fn branch_name(repo: &DocRepo, id: &str) -> String {
    let prefix = repo.branch_prefix.clone().unwrap_or_else(|| "doc/".to_owned());
    format!("{prefix}{id}")
}

/// Persist a mutated `repo` linkage onto the document (keeping every other field).
async fn save_repo(state: &AppState, mut doc: Document, repo: DocRepo) -> ApiResult<Document> {
    doc.repo = Some(repo);
    Ok(state.store.update_document(&doc).await?)
}

/// `PUT /documents/:id/repo` — set the publishing target. Requires `Document`.
pub async fn set_repo(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<SetDocRepoBody>,
) -> ApiResult<Json<Document>> {
    session.require(Capability::Document, "document", &id)?;
    let doc = require_document(&state, &id).await?;
    // Preserve any already-filled linkage (branch/urls) if a repo was set before.
    let prior = doc.repo.clone().unwrap_or_default();
    let repo = DocRepo {
        repo: body.repo,
        base_branch: body.base_branch,
        branch_prefix: body.branch_prefix,
        output_path: body.output_path,
        branch: prior.branch,
        issue_url: prior.issue_url,
        pr_url: prior.pr_url,
    };
    Ok(Json(save_repo(&state, doc, repo).await?))
}

/// `POST /documents/:id/gh/branch` — create the document's branch off its base.
/// Requires `Document`.
pub async fn create_branch(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> ApiResult<Json<BranchOut>> {
    session.require(Capability::Document, "document", &id)?;
    let doc = require_document(&state, &id).await?;
    let mut repo = repo_of(&doc)?;
    let name = branch_name(&repo, &id);
    let gh = Gh::new();
    gh.create_branch(&repo.repo, &name, repo.base_branch.as_deref())
        .await?;
    let branch = gh.current_branch(&repo.repo).await?;
    repo.branch = Some(branch.clone());
    save_repo(&state, doc, repo).await?;
    Ok(Json(BranchOut { branch }))
}

/// `POST /documents/:id/gh/commit` — render the doc, write it to `output_path`,
/// `git add`/`commit`, optionally push. Requires `Document`.
pub async fn commit(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<CommitBody>,
) -> ApiResult<Json<CommitOut>> {
    session.require(Capability::Document, "document", &id)?;
    let doc = require_document(&state, &id).await?;
    let repo = repo_of(&doc)?;
    let markdown = assemble_markdown(&state, &doc).await?;

    // Write the rendered markdown into the repo at the configured output path.
    let dest = FsPath::new(&repo.repo).join(&repo.output_path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::Internal(format!("create output dir: {e}")))?;
    }
    std::fs::write(&dest, markdown.as_bytes())
        .map_err(|e| ApiError::Internal(format!("write output file: {e}")))?;

    let gh = Gh::new();
    gh.git(&repo.repo, ["add", repo.output_path.as_str()]).await?;
    let message = body
        .message
        .unwrap_or_else(|| format!("docs: update {}", doc.title));
    gh.git(&repo.repo, ["commit", "-m", message.as_str()]).await?;

    let pushed = if body.push {
        let branch = match &repo.branch {
            Some(b) => b.clone(),
            None => gh.current_branch(&repo.repo).await?,
        };
        gh.git(&repo.repo, ["push", "-u", "origin", branch.as_str()])
            .await?;
        true
    } else {
        false
    };

    Ok(Json(CommitOut {
        committed: true,
        pushed,
        output_path: repo.output_path,
    }))
}

/// `POST /documents/:id/gh/pr` — open a PR for the document's branch. Requires
/// `Document`. Persists `pr_url`.
pub async fn create_pr(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<PrBody>,
) -> ApiResult<Json<UrlOut>> {
    session.require(Capability::Document, "document", &id)?;
    let doc = require_document(&state, &id).await?;
    let mut repo = repo_of(&doc)?;
    let head = repo
        .branch
        .clone()
        .ok_or_else(|| ApiError::bad_request("document has no branch yet; POST gh/branch first"))?;
    let base = repo
        .base_branch
        .clone()
        .unwrap_or_else(|| "main".to_owned());
    let title = body.title.unwrap_or_else(|| doc.title.clone());
    let pr_body = body
        .body
        .unwrap_or_else(|| format!("Rendered document `{}`.", doc.id));
    let url = Gh::new()
        .pr_create(&repo.repo, &title, &pr_body, &head, &base, body.draft)
        .await?;
    repo.pr_url = Some(url.clone());
    save_repo(&state, doc, repo).await?;
    Ok(Json(UrlOut { url }))
}

/// `POST /documents/:id/gh/issue` — open an issue from the document. Requires
/// `Document`. Persists `issue_url`.
pub async fn create_issue(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<IssueBody>,
) -> ApiResult<Json<UrlOut>> {
    session.require(Capability::Document, "document", &id)?;
    let doc = require_document(&state, &id).await?;
    let mut repo = repo_of(&doc)?;
    let title = body.title.unwrap_or_else(|| doc.title.clone());
    let issue_body = body.body.unwrap_or_else(|| doc.body.clone());
    let url = Gh::new()
        .issue_create(&repo.repo, &title, &issue_body)
        .await?;
    repo.issue_url = Some(url.clone());
    save_repo(&state, doc, repo).await?;
    Ok(Json(UrlOut { url }))
}

/// `POST /documents/:id/publish` — branch → commit (push) → PR, in one call.
/// Requires `Document`. Persists `branch` + `pr_url`.
pub async fn publish(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<PublishBody>,
) -> ApiResult<Json<PublishOut>> {
    session.require(Capability::Document, "document", &id)?;
    let doc = require_document(&state, &id).await?;
    let mut repo = repo_of(&doc)?;
    let gh = Gh::new();

    // 1) Branch off the base.
    let name = branch_name(&repo, &id);
    gh.create_branch(&repo.repo, &name, repo.base_branch.as_deref())
        .await?;
    let branch = gh.current_branch(&repo.repo).await?;
    repo.branch = Some(branch.clone());

    // 2) Render + write + commit + push.
    let markdown = assemble_markdown(&state, &doc).await?;
    let dest = FsPath::new(&repo.repo).join(&repo.output_path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::Internal(format!("create output dir: {e}")))?;
    }
    std::fs::write(&dest, markdown.as_bytes())
        .map_err(|e| ApiError::Internal(format!("write output file: {e}")))?;
    gh.git(&repo.repo, ["add", repo.output_path.as_str()]).await?;
    let message = body
        .message
        .unwrap_or_else(|| format!("docs: publish {}", doc.title));
    gh.git(&repo.repo, ["commit", "-m", message.as_str()]).await?;
    gh.git(&repo.repo, ["push", "-u", "origin", branch.as_str()])
        .await?;

    // 3) Open the PR.
    let base = repo
        .base_branch
        .clone()
        .unwrap_or_else(|| "main".to_owned());
    let title = body.title.unwrap_or_else(|| doc.title.clone());
    let pr_body = body
        .body
        .unwrap_or_else(|| format!("Rendered document `{}`.", doc.id));
    let url = gh
        .pr_create(&repo.repo, &title, &pr_body, &branch, &base, false)
        .await?;
    repo.pr_url = Some(url.clone());

    save_repo(&state, doc, repo).await?;
    Ok(Json(PublishOut {
        branch,
        pr_url: url,
    }))
}
