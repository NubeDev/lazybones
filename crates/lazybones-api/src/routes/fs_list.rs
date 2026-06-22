//! `GET /fs/list?path=/abs/dir` — browse the host filesystem for the UI's
//! repo/dir selector (the "New workflow → Repo" field wants a native picker).
//!
//! Unguarded like `/engine`/`/health`: it only lists directory names a local
//! operator could already `ls`, and the desktop UI needs it before any token
//! exists. It returns subdirectories only (you pick a *repo dir*), each flagged
//! with whether it looks like a git repo, plus the resolved absolute path and
//! parent so the UI can walk up/down.

use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::Query;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};

/// `?path=` — the directory to list. Omitted/empty ⇒ `$HOME` (or `/`).
#[derive(Debug, Deserialize)]
pub struct FsQuery {
    path: Option<String>,
}

/// One browsable child directory.
#[derive(Debug, Serialize)]
pub struct DirEntry {
    /// Directory name (last path component).
    pub name: String,
    /// Absolute path, for the next `?path=` step or to use as the repo.
    pub path: String,
    /// Whether this dir contains a `.git` — i.e. is itself a git repo.
    pub is_repo: bool,
}

/// Listing of one directory: where we are, where "up" goes, and the children.
#[derive(Debug, Serialize)]
pub struct FsListing {
    /// The resolved absolute path we listed.
    pub path: String,
    /// Parent directory, or `None` at the filesystem root.
    pub parent: Option<String>,
    /// Whether `path` itself is a git repo (so the UI can offer "use this dir").
    pub is_repo: bool,
    /// Child directories, sorted, hidden dirs excluded.
    pub entries: Vec<DirEntry>,
}

/// List directories under `path` (default `$HOME`). Subdirectories only.
pub async fn fs_list(Query(query): Query<FsQuery>) -> ApiResult<Json<FsListing>> {
    let base = match query.path.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => home_dir(),
    };

    // Resolve symlinks/`..` so `path`/`parent` are canonical and predictable.
    let dir = base
        .canonicalize()
        .map_err(|e| ApiError::Internal(format!("cannot open {}: {e}", base.display())))?;
    if !dir.is_dir() {
        return Err(ApiError::Internal(format!(
            "{} is not a directory",
            dir.display()
        )));
    }

    let mut entries = Vec::new();
    let mut read = std::fs::read_dir(&dir)
        .map_err(|e| ApiError::Internal(format!("cannot read {}: {e}", dir.display())))?;
    while let Some(Ok(item)) = read.next() {
        if !item.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue; // skip dotfiles/dirs in the picker
        }
        let path = item.path();
        let is_repo = is_git_repo(&path);
        entries.push(DirEntry {
            name,
            path: path.to_string_lossy().into_owned(),
            is_repo,
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(FsListing {
        is_repo: is_git_repo(&dir),
        parent: dir.parent().map(|p| p.to_string_lossy().into_owned()),
        path: dir.to_string_lossy().into_owned(),
        entries,
    }))
}

/// `$HOME`, falling back to `/` if unset.
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// A directory is a git repo if it holds a `.git` (dir for a clone, file for a
/// worktree/submodule).
fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}
