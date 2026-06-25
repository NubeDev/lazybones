//! Error mapping for the MCP surface — the twin of [`lazybones-api`'s
//! `error.rs`](../../../crates/lazybones-api/src/error.rs).
//!
//! Tools fail the same way routes do: a missing capability or bad token is a
//! permission/`invalid_request` error, a missing record is "not found", an illegal
//! transition or duplicate is a conflict (`invalid_request`), and anything else is
//! an internal error. We funnel those through one [`McpError`] enum and convert it
//! into rmcp's wire [`ErrorData`] so the JSON-RPC fault payload matches the REST
//! status code the same failure would produce.

use lazybones_auth::AuthError;
use lazybones_store::{AssetError, StoreError};
use rmcp::ErrorData;

/// Convenience alias for tool results: `Ok(T)` or a wire-ready [`ErrorData`].
pub type McpResult<T> = Result<T, ErrorData>;

/// A failure surfaced from an MCP tool, before it is lowered to the wire
/// [`ErrorData`]. Mirrors `lazybones-api::ApiError` so the two surfaces classify
/// the same domain failure identically.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// No or malformed bearer token (REST `401`).
    #[error("unauthorized")]
    Unauthorized,

    /// The session lacks the required capability or task scope (REST `403`).
    #[error(transparent)]
    Forbidden(#[from] AuthError),

    /// A store-boundary failure — classified into not-found / conflict / internal
    /// below, matching the REST mapping.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// A blob-store (asset bytes) failure.
    #[error(transparent)]
    Asset(#[from] AssetError),

    /// A `gh`/`git` failure from a document publish action (REST maps this to
    /// `502 Bad Gateway`; MCP has no gateway fault, so it is an internal error).
    #[error(transparent)]
    Gh(#[from] lazybones_gh::GhError),

    /// The requested resource does not exist (REST `404`).
    #[error("not found")]
    NotFound,

    /// The request is well-formed but semantically rejected (REST `400`).
    #[error("{0}")]
    BadRequest(String),

    /// The request conflicts with the resource's current state (REST `409`).
    #[error("{0}")]
    Conflict(String),

    /// An unexpected server-side failure (REST `500`).
    #[error("{0}")]
    Internal(String),
}

impl McpError {
    /// A bad-request error with a human-readable reason.
    pub fn bad_request(msg: impl Into<String>) -> Self {
        McpError::BadRequest(msg.into())
    }

    /// A conflict error with a human-readable reason.
    pub fn conflict(msg: impl Into<String>) -> Self {
        McpError::Conflict(msg.into())
    }
}

impl From<McpError> for ErrorData {
    fn from(err: McpError) -> Self {
        let message = err.to_string();
        match err {
            // Auth/permission and malformed/rejected requests map to the
            // `invalid_request` / `invalid_params` JSON-RPC faults.
            McpError::Unauthorized | McpError::Forbidden(_) => {
                ErrorData::invalid_request(message, None)
            }
            McpError::BadRequest(_) => ErrorData::invalid_params(message, None),
            McpError::Conflict(_) => ErrorData::invalid_request(message, None),
            // Missing records (whether named via `NotFound` or surfaced from the
            // store/blob layer) map to `resource_not_found`.
            McpError::NotFound
            | McpError::Store(
                StoreError::TaskNotFound(_)
                | StoreError::TemplateNotFound(_)
                | StoreError::SkillNotFound(_)
                | StoreError::RunNotFound(_)
                | StoreError::AgentNotFound(_)
                | StoreError::DocumentNotFound(_)
                | StoreError::AssetNotFound(_)
                | StoreError::BrandingNotFound(_)
                | StoreError::SourceNotFound(_)
                | StoreError::ExtensionNotFound(_),
            )
            | McpError::Asset(AssetError::NotFound(_)) => {
                ErrorData::resource_not_found(message, None)
            }
            // Illegal transitions and duplicate ids are conflicts.
            McpError::Store(
                StoreError::IllegalTransition { .. }
                | StoreError::TaskExists(_)
                | StoreError::TemplateExists(_)
                | StoreError::SkillExists(_)
                | StoreError::RunExists(_)
                | StoreError::AgentExists(_)
                | StoreError::DocumentExists(_)
                | StoreError::BrandingExists(_)
                | StoreError::ExtensionExists(_),
            ) => ErrorData::invalid_request(message, None),
            // Anything else from the store/blob layer — and a `gh`/`git` publish
            // failure — is ours.
            McpError::Store(_) | McpError::Asset(_) | McpError::Gh(_) | McpError::Internal(_) => {
                ErrorData::internal_error(message, None)
            }
        }
    }
}
