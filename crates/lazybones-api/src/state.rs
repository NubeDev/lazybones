//! Application state shared across every request handler.
//!
//! Holds the durable [`StoreHandle`] and the token registry that maps a bearer
//! token to the [`ScopedSession`] it authenticates. Cloneable (an `Arc` bump) so
//! axum can share it across handlers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

use lazybones_auth::{ManagementProfile, ScopedSession};
use lazybones_ext::{EngineLimits, ExtEngine, Registry};
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
    /// The in-memory extension registry — the dispatch index keyed by exported WIT
    /// interface (extension-system design §3.2). The `/extensions` routes install
    /// into it and mirror enable/disable/grant decisions here so the scheduler's
    /// dispatch stays in lock-step with the durable store rows. Behind an `RwLock`
    /// because install/remove mutate it. The store remains authoritative; this is
    /// rebuilt from it on boot.
    extensions: Arc<RwLock<Registry>>,
    /// The shared Wasmtime extension engine (one per process, fuel+epoch+memory
    /// limited — design §3.4), built lazily on first test-invoke so the common
    /// no-extensions path (and most tests) never pays the engine/ticker cost.
    ext_engine: Arc<OnceLock<ExtEngine>>,
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
            extensions: Arc::new(RwLock::new(Registry::new())),
            ext_engine: Arc::new(OnceLock::new()),
        }
    }

    /// The shared extension registry (the dispatch index). Cheap to clone (an
    /// `Arc` bump); the `/extensions` routes lock it to install/remove and to
    /// mirror enable/disable/grant decisions.
    #[must_use]
    pub fn extensions(&self) -> &Arc<RwLock<Registry>> {
        &self.extensions
    }

    /// The shared Wasmtime extension engine, built on first use. A second-or-later
    /// call returns the already-initialised engine. Engine construction failure is
    /// a process-level environment fault (a bad Wasmtime config), so it panics
    /// rather than threading a `Result` through every caller.
    #[must_use]
    pub fn ext_engine(&self) -> &ExtEngine {
        self.ext_engine.get_or_init(|| {
            ExtEngine::new(EngineLimits::default())
                .expect("initialize the shared wasm extension engine")
        })
    }

    /// The shared, lazily-built extension engine **cell** (rather than the resolved
    /// engine). Handed to the in-process MCP server at mount so it resolves the
    /// *same* engine on first `extension.invoke` without forcing construction (and
    /// its epoch ticker thread) at boot — the engine is built once, shared across
    /// both front doors.
    #[must_use]
    pub fn ext_engine_cell(&self) -> &Arc<OnceLock<ExtEngine>> {
        &self.ext_engine
    }

    /// Swap in the asset blob store (builder style). Called by `serve.rs` to root
    /// the store at the daemon's `data_dir`; tests use it to isolate blob bytes in
    /// a tempdir.
    #[must_use]
    pub fn with_assets(mut self, assets: AssetStore) -> Self {
        self.assets = assets;
        self
    }

    /// Share an externally-built extension registry **and** engine (builder style).
    ///
    /// Called by `serve.rs` so the `/extensions` routes and the scheduler's
    /// [`Dispatcher`](lazybones_ext::Dispatcher) operate on the **same** registry —
    /// an install/enable/disable/grant via REST is immediately visible to gate-check
    /// dispatch, and a breaker auto-disable is visible to the API. The store stays
    /// authoritative; both are rebuilt from it on boot. The pre-built engine is also
    /// shared so the process has exactly one Wasmtime engine + epoch ticker.
    #[must_use]
    pub fn with_ext_runtime(mut self, registry: Arc<RwLock<Registry>>, engine: ExtEngine) -> Self {
        self.extensions = registry;
        // `set` only fails if already initialised; a fresh `AppState` never is.
        let _ = self.ext_engine.set(engine);
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

/// Let the in-process MCP server (`lazybones-mcp`) resolve a bearer token against
/// this same token registry, so an MCP connection authenticates exactly like a REST
/// request — the MCP surface is a second front door onto the existing grants, not a
/// new auth plane (docs/mcp/README.md §3).
impl lazybones_mcp::SessionResolver for AppState {
    fn session_for(&self, token: &str) -> Option<ScopedSession> {
        AppState::session_for(self, token)
    }
}
