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
//! out of an `Authorization` header value, resolving it to a session through the
//! [`SessionResolver`] the mount supplies, and the [`require`] capability guard.

use lazybones_auth::{AuthError, Capability, ScopedSession};

use crate::error::McpError;

/// Resolve a bearer token to the [`ScopedSession`] it authenticates.
///
/// The MCP server runs in-process inside `lazybonesd`; the token → session registry
/// lives on `lazybones-api`'s `AppState` (`AppState::session_for`). To keep this
/// crate free of a dependency cycle back onto the API, that registry is reached
/// through this trait: `AppState` implements it by delegating to its inherent
/// `session_for`, so an MCP connection authenticates against the **same** map a REST
/// request does — the MCP surface is a second front door onto the existing grants,
/// not a new auth plane (design §3).
pub trait SessionResolver: Send + Sync {
    /// Map a bearer token to its session, or `None` if the token is unregistered.
    fn session_for(&self, token: &str) -> Option<ScopedSession>;
}

/// Resolve the [`ScopedSession`] an MCP request acts under from its `Authorization`
/// header value, looking the bearer token up via `resolver`.
///
/// Returns `None` when the header is absent/malformed or the token is unregistered —
/// the unauthenticated read-only path (no token ⇒ only the unguarded read tools
/// resolve; every mutator then refuses via [`require`], design §3).
#[must_use]
pub fn resolve_session(
    resolver: &dyn SessionResolver,
    authorization: Option<&str>,
) -> Option<ScopedSession> {
    let token = bearer_token(authorization?)?;
    resolver.session_for(token)
}

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

    /// A tiny in-test registry so `resolve_session` can be exercised without the
    /// API's `AppState`.
    struct FakeRegistry(Option<(String, ScopedSession)>);
    impl SessionResolver for FakeRegistry {
        fn session_for(&self, token: &str) -> Option<ScopedSession> {
            self.0
                .as_ref()
                .filter(|(t, _)| t == token)
                .map(|(_, s)| s.clone())
        }
    }

    #[test]
    fn resolve_session_maps_bearer_then_falls_through_to_none() {
        let session = ScopedSession::for_management(
            "tester",
            lazybones_auth::ManagementProfile::Author,
        );
        let reg = FakeRegistry(Some(("secret".to_owned(), session)));

        // A registered bearer token resolves to its session.
        assert!(resolve_session(&reg, Some("Bearer secret")).is_some());
        // An unknown token, a non-bearer scheme, and a missing header all fall
        // through to the unauthenticated read-only path.
        assert!(resolve_session(&reg, Some("Bearer nope")).is_none());
        assert!(resolve_session(&reg, Some("Basic secret")).is_none());
        assert!(resolve_session(&reg, None).is_none());
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
