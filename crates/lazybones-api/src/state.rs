//! Application state shared across every request handler.
//!
//! Holds the durable [`StoreHandle`] and the token registry that maps a bearer
//! token to the [`ScopedSession`] it authenticates. Cloneable (an `Arc` bump) so
//! axum can share it across handlers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use lazybones_auth::{ManagementProfile, ScopedSession};
use lazybones_store::{BlobStore, FileBlobStore, StoreHandle};

/// The content-addressed byte store for asset payloads, behind the
/// [`BlobStore`](lazybones_store::BlobStore) trait so the backend (files today,
/// S3/bucket later) is swappable without touching routes. Shared (`Arc`) so the
/// cloneable [`AppState`] stays cheap to clone.
pub type AssetStore = Arc<dyn BlobStore>;

/// Shared state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    /// The durable store boundary.
    pub store: StoreHandle,
    /// The blob store holding asset bytes (logos/images) outside the relational
    /// rows. Constructed in `serve.rs` from `config.data_dir`.
    pub assets: AssetStore,
    /// The run label this server serves (groups history).
    pub run: String,
    /// The externally-reachable base URL of this REST API (e.g.
    /// `http://127.0.0.1:8080`). Handed to the management agent so it can call
    /// the API the same way an operator does.
    pub base_url: String,
    /// A monotonic counter for minting unique management-agent token suffixes.
    /// (`Date.now`/random are avoided; a simple counter is enough here.)
    mint_counter: Arc<AtomicU64>,
    /// Bearer-token → session registry. The loop token is seeded at boot; agent
    /// tokens are minted on claim. Behind an `RwLock` so claim can register one.
    tokens: Arc<RwLock<HashMap<String, ScopedSession>>>,
}

impl AppState {
    /// Build state around a store handle, run label, base URL, and the loop's
    /// bearer token.
    #[must_use]
    pub fn new(
        store: StoreHandle,
        run: impl Into<String>,
        base_url: impl Into<String>,
        loop_token: impl Into<String>,
    ) -> Self {
        let mut tokens = HashMap::new();
        tokens.insert(loop_token.into(), ScopedSession::for_loop("loop"));
        Self {
            store,
            // Default to a file blob store under the OS temp dir; `serve.rs` swaps
            // in the data-dir-rooted store via [`with_assets`](Self::with_assets).
            assets: Arc::new(FileBlobStore::new(std::env::temp_dir().join("lazybones-assets"))),
            run: run.into(),
            base_url: base_url.into(),
            mint_counter: Arc::new(AtomicU64::new(0)),
            tokens: Arc::new(RwLock::new(tokens)),
        }
    }

    /// Swap in the asset blob store (builder style). Called by `serve.rs` to root
    /// the store at the daemon's `data_dir`; tests use it to isolate blob bytes in
    /// a tempdir.
    #[must_use]
    pub fn with_assets(mut self, assets: AssetStore) -> Self {
        self.assets = assets;
        self
    }

    /// Mint and register a scoped management-agent token for a conversation,
    /// returning the bearer string. The grant comes from the permission profile
    /// (read-only ⇒ `[Read]`, author ⇒ `[Read, Author]`, manage ⇒
    /// `[Read, Author, Block]`); it is task-unbound but never carries
    /// `Claim`/`Secret` (`docs/agent/lazybones-agent-scope.md` §10).
    ///
    /// Tokens are minted per turn and remain registered for the process
    /// lifetime; the conversation id keeps the actor auditable.
    pub fn mint_management_token(
        &self,
        conversation_id: &str,
        profile: ManagementProfile,
    ) -> String {
        let n = self.mint_counter.fetch_add(1, Ordering::Relaxed);
        let token = format!("lazybones-agent-{conversation_id}-{n}");
        let actor = format!("agent:{conversation_id}");
        self.register_agent(token.clone(), ScopedSession::for_management(actor, profile));
        token
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
