//! HTTP error mapping for the REST surface.
//!
//! Turns store/auth failures into status codes a client can act on: an illegal
//! transition or missing task is the caller's fault (`409`/`404`); a missing
//! capability or bad token is `401`/`403`; anything else is `500`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lazybones_auth::AuthError;
use lazybones_store::StoreError;
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
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            ApiError::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
            ApiError::Store(StoreError::TaskNotFound(_)) => {
                (StatusCode::NOT_FOUND, self.to_string())
            }
            ApiError::Store(StoreError::IllegalTransition { .. }) => {
                (StatusCode::CONFLICT, self.to_string())
            }
            ApiError::Store(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

/// Convenience alias for handler results.
pub type ApiResult<T> = Result<T, ApiError>;
