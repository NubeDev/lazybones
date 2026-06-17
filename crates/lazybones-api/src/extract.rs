//! The session extractor: resolve a bearer token to a [`ScopedSession`].
//!
//! Every mutating route takes a [`Session`] argument; axum runs this extractor
//! first, so a request with no/unknown token is rejected `401` before the handler
//! body runs. The handler then asserts the specific capability + task scope it
//! needs via [`Session::require`].

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use lazybones_auth::{AuthError, Capability, ScopedSession};

use crate::error::ApiError;
use crate::state::AppState;

/// A resolved session, ready for capability checks inside a handler.
pub struct Session(pub ScopedSession);

#[axum::async_trait]
impl FromRequestParts<AppState> for Session {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ApiError::Unauthorized)?;
        let session = state.session_for(token).ok_or(ApiError::Unauthorized)?;
        Ok(Session(session))
    }
}

impl Session {
    /// Assert this session holds `cap` and may act on task `id`.
    ///
    /// # Errors
    /// Returns [`ApiError::Forbidden`] if the capability is missing or the
    /// session is bound to a different task.
    pub fn require(&self, cap: Capability, name: &'static str, id: &str) -> Result<(), ApiError> {
        if !self.0.can(cap) {
            return Err(AuthError::MissingCapability(name).into());
        }
        if !self.0.may_act_on(id) {
            return Err(AuthError::WrongTask(id.to_owned()).into());
        }
        Ok(())
    }

    /// The actor name to record on an event driven by this session.
    #[must_use]
    pub fn actor(&self) -> &str {
        self.0.actor()
    }
}
