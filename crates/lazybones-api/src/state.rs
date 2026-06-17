//! Application state shared across every request handler.
//!
//! Holds the durable [`StoreHandle`] and the token registry that maps a bearer
//! token to the [`ScopedSession`] it authenticates. Cloneable (an `Arc` bump) so
//! axum can share it across handlers.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use lazybones_auth::ScopedSession;
use lazybones_store::StoreHandle;

/// Shared state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    /// The durable store boundary.
    pub store: StoreHandle,
    /// The run label this server serves (groups history).
    pub run: String,
    /// Bearer-token → session registry. The loop token is seeded at boot; agent
    /// tokens are minted on claim. Behind an `RwLock` so claim can register one.
    tokens: Arc<RwLock<HashMap<String, ScopedSession>>>,
}

impl AppState {
    /// Build state around a store handle, run label, and the loop's bearer token.
    #[must_use]
    pub fn new(store: StoreHandle, run: impl Into<String>, loop_token: impl Into<String>) -> Self {
        let mut tokens = HashMap::new();
        tokens.insert(loop_token.into(), ScopedSession::for_loop("loop"));
        Self {
            store,
            run: run.into(),
            tokens: Arc::new(RwLock::new(tokens)),
        }
    }

    /// Resolve a bearer token to its session, if registered.
    #[must_use]
    pub fn session_for(&self, token: &str) -> Option<ScopedSession> {
        self.tokens
            .read()
            .expect("token registry lock poisoned")
            .get(token)
            .cloned()
    }

    /// Register an agent token bound to a task (called on claim).
    pub fn register_agent(&self, token: impl Into<String>, session: ScopedSession) {
        self.tokens
            .write()
            .expect("token registry lock poisoned")
            .insert(token.into(), session);
    }
}
