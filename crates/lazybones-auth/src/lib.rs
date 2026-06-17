//! Scoped sessions + capability grants for lazybones.
//!
//! A small, honest authorization boundary: the trusted loop gets every
//! capability; an agent session is scoped to its one task and the capabilities it
//! needs to drive that task and write memory (SCOPE.md, `auth`).

mod capability;
mod error;
mod session;

pub use capability::Capability;
pub use error::AuthError;
pub use session::ScopedSession;
