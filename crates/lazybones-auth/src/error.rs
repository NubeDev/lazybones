//! Auth-domain errors.

/// Why a scoped session was refused an action.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AuthError {
    /// The session lacks the capability the route requires.
    #[error("missing capability: {0}")]
    MissingCapability(&'static str),

    /// The session is bound to a different task than the one it tried to act on.
    #[error("session may not act on task {0}")]
    WrongTask(String),

    /// The principal holds a valid capability but lacks the org-graph role the
    /// verb demands (admin / team-manager / member — projects.md "Roles"). The
    /// string explains which authority was required.
    #[error("forbidden: {0}")]
    ForbiddenRole(String),
}
