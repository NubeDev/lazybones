//! HTTP error mapping for the REST surface.
//!
//! Turns store/auth failures into status codes a client can act on: an illegal
//! transition or missing task is the caller's fault (`409`/`404`); a missing
//! capability or bad token is `401`/`403`; anything else is `500`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lazybones_auth::AuthError;
use lazybones_store::{AssetError, StoreError};
use serde_json::json;

/// An error surfaced from a route handler.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// No or malformed bearer token.
    #[error("unauthorized")]
    Unauthorized,

    /// The session lacks the required capability or task scope.
    #[error(transparent)]
    Forbidden(#[from] AuthError),

    /// A store-boundary failure (mapped by status below).
    #[error(transparent)]
    Store(#[from] StoreError),

    /// The requested resource doesn't exist (e.g. an unknown agent tool).
    #[error("not found")]
    NotFound,

    /// A `gh`/`git` CLI invocation failed (not installed, not authed, or the
    /// command itself errored). Surfaced as `502` — the failure is upstream of
    /// us, in a tool we shell out to.
    #[error(transparent)]
    Gh(#[from] lazybones_gh::GhError),

    /// A blob-store (asset bytes) failure: a missing blob is `404`, an IO error
    /// is `500` (mapped below), mirroring the `StoreError` story.
    #[error(transparent)]
    Asset(#[from] AssetError),

    /// A document failed to render to PDF (Typst compile/export error). Ours to
    /// own — surfaced as `500`.
    #[error(transparent)]
    Render(#[from] lazybones_render::RenderError),

    /// The request is well-formed but semantically rejected (e.g. trying to
    /// remove the main worktree). `400`.
    #[error("{0}")]
    BadRequest(String),

    /// The request conflicts with the current state of the resource (e.g.
    /// deleting a workflow that still has running tasks). `409`.
    #[error("{0}")]
    Conflict(String),

    /// An unexpected server-side failure.
    #[error("{0}")]
    Internal(String),
}

/// Map an engine [`IssueError`](lazybones_engine::IssueError) onto an HTTP
/// status: a standalone task or a bad link is the caller's fault (`400`); a
/// missing task / unlinked task is `404`; an unavailable `gh` is upstream
/// (`502`); a store failure reuses the store mapping.
impl From<lazybones_engine::IssueError> for ApiError {
    fn from(e: lazybones_engine::IssueError) -> Self {
        use lazybones_engine::IssueError as E;
        match e {
            E::Standalone(_) | E::BadLink(_) => ApiError::BadRequest(e.to_string()),
            E::TaskNotFound(_) | E::NotLinked(_) => ApiError::NotFound,
            // `gh` missing / unauthenticated is an actionable env problem ("run
            // `gh auth login`") — surface the message verbatim as a `400`.
            E::Auth(_) => ApiError::BadRequest(e.to_string()),
            E::Gh(g) => ApiError::Gh(g),
            E::Store(s) => ApiError::Store(s),
        }
    }
}

impl ApiError {
    /// A `400 Bad Request` with a human-readable reason.
    pub fn bad_request(msg: impl Into<String>) -> Self {
        ApiError::BadRequest(msg.into())
    }

    /// A `409 Conflict` with a human-readable reason.
    pub fn conflict(msg: impl Into<String>) -> Self {
        ApiError::Conflict(msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            ApiError::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
            ApiError::Store(
                StoreError::TaskNotFound(_)
                | StoreError::TemplateNotFound(_)
                | StoreError::SkillNotFound(_)
                | StoreError::RunNotFound(_)
                | StoreError::AgentNotFound(_)
                | StoreError::DocumentNotFound(_)
                | StoreError::AssetNotFound(_)
                | StoreError::BrandingNotFound(_)
                | StoreError::SourceNotFound(_),
            ) => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::Store(StoreError::IllegalTransition { .. }) => {
                (StatusCode::CONFLICT, self.to_string())
            }
            ApiError::Store(
                StoreError::TaskExists(_)
                | StoreError::TemplateExists(_)
                | StoreError::SkillExists(_)
                | StoreError::RunExists(_)
                | StoreError::AgentExists(_)
                | StoreError::DocumentExists(_)
                | StoreError::BrandingExists(_),
            ) => (StatusCode::CONFLICT, self.to_string()),
            ApiError::Store(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::Gh(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            // Asset bytes: a missing blob is the caller's fault (`404`); an IO
            // failure is ours (`500`).
            ApiError::Asset(AssetError::NotFound(_)) => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::Asset(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::Render(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

/// Convenience alias for handler results.
pub type ApiResult<T> = Result<T, ApiError>;
