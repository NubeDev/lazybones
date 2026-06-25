//! Authentication & per-tool capability gating for the MCP surface.
//!
//! An MCP connection authenticates exactly like a REST request: a bearer token in
//! the transport's `Authorization` header, resolved to a
//! [`ScopedSession`](lazybones_auth::ScopedSession) (the resolution itself —
//! token → session — is `AppState::session_for`, wired when the server is mounted at
//! `/mcp`, task `mcp-mount`). Every mutating tool then re-checks the session's
//! capability with [`require`] before acting — the same `session.can(cap)` gate the
//! routes use. No token ⇒ only the unguarded read tools resolve (design §3).
//!
//! This module owns the pure, mount-independent pieces: parsing the bearer token
//! out of an `Authorization` header value, and the [`require`] capability guard.

use lazybones_auth::{AuthError, Capability, ScopedSession};

use crate::error::McpError;

/// Extract the bearer token from an `Authorization` header value, if present and
/// well-formed (`"Bearer <token>"`, case-insensitive scheme, non-empty token).
///
/// Returns `None` for a missing/blank token so the caller can fall through to the
/// unauthenticated read-only path (mirroring "GET reads are open").
#[must_use]
pub fn bearer_token(authorization: &str) -> Option<&str> {
    let (scheme, token) = authorization.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// Require that `session` holds `cap`, returning the capability's wire name in a
/// [`McpError::Forbidden`] otherwise. This is the per-tool guard every mutator
/// calls before touching the store — the MCP twin of the routes' `session.can(cap)`
/// check, so the two surfaces enforce the *same* grants (design §3).
///
/// # Errors
///
/// Returns [`McpError::Forbidden`] carrying [`AuthError::MissingCapability`] when
/// the session lacks `cap`.
pub fn require(session: &ScopedSession, cap: Capability) -> Result<(), McpError> {
    if session.can(cap) {
        Ok(())
    } else {
        Err(McpError::Forbidden(AuthError::MissingCapability(
            capability_name(cap),
        )))
    }
}

/// A stable, human-readable name for a capability, used in the refusal message so
/// a client sees *which* grant it is missing.
const fn capability_name(cap: Capability) -> &'static str {
    match cap {
        Capability::Sync => "sync",
        Capability::Claim => "claim",
        Capability::Heartbeat => "heartbeat",
        Capability::Done => "done",
        Capability::Block => "block",
        Capability::Memorize => "memorize",
        Capability::Read => "read",
        Capability::Secret => "secret",
        Capability::Author => "author",
        Capability::Document => "document",
        Capability::Extension => "extension",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bearer_token_case_insensitively() {
        assert_eq!(bearer_token("Bearer abc123"), Some("abc123"));
        assert_eq!(bearer_token("bearer abc123"), Some("abc123"));
        assert_eq!(bearer_token("BEARER  spaced  "), Some("spaced"));
    }

    #[test]
    fn rejects_malformed_or_empty_authorization() {
        assert_eq!(bearer_token("Basic abc123"), None);
        assert_eq!(bearer_token("abc123"), None);
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token(""), None);
    }

    #[test]
    fn require_passes_when_granted_and_refuses_otherwise() {
        let session = ScopedSession::for_management(
            "tester",
            lazybones_auth::ManagementProfile::Author,
        );
        // Author profile holds Author + Document + Read, never Claim.
        assert!(require(&session, Capability::Author).is_ok());
        assert!(require(&session, Capability::Read).is_ok());
        assert!(matches!(
            require(&session, Capability::Claim),
            Err(McpError::Forbidden(AuthError::MissingCapability("claim")))
        ));
    }
}
