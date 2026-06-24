//! Open the store and either serve the REST API or run a one-shot import.

use std::path::Path;

use std::sync::{Arc, RwLock};

use lazybones_api::{AppState, router};
use lazybones_engine::{BlobComponentLoader, EngineConfig, ExtHooks};
use lazybones_ext::{Dispatcher, DispatcherConfig, EngineLimits, ExtEngine, Registry};
use lazybones_store::{BlobStore, FileBlobStore, StoreEngine, StoreHandle, sync_seeds};

use crate::configure::Config;
use crate::workfile::parse_workfile;

/// Open the embedded store at the configured data dir, namespace, and database.
async fn open_store(config: &Config) -> anyhow::Result<StoreHandle> {
    let engine = StoreEngine::File {
        path: config.data_dir.clone(),
    };
    let store =
        StoreHandle::open(&engine, &config.namespace, &config.database, &config.secret_key).await?;
    // Seed the bundled default agent catalog on every boot. It's idempotent and
    // never clobbers operator edits, so a fresh install gets a usable catalog and
    // an existing one is left exactly as the operator left it.
    match store.seed_default_agents(&store.now()).await {
        Ok(n) if n > 0 => tracing::info!(seeded = n, "agent catalog seeded with defaults"),
        Ok(_) => {}
        Err(e) => tracing::warn!("agent catalog seed failed: {e}"),
    }
    // Seed a few demo skills the same way: idempotent, non-clobbering, so a fresh
    // install has starter recipes to attach to templates.
    match store.seed_default_skills(&store.now()).await {
        Ok(n) if n > 0 => tracing::info!(seeded = n, "skill catalogue seeded with demos"),
        Ok(_) => {}
        Err(e) => tracing::warn!("skill catalogue seed failed: {e}"),
    }
    Ok(store)
}

/// Serve the REST API and run the in-process scheduler until signalled.
///
/// The scheduler shares the same [`StoreHandle`] — it reads/writes the store
/// in-process, no HTTP round-trip to itself — and is aborted when the API stops.
///
/// # Errors
/// Returns an error if the store cannot open or the listener cannot bind.
pub async fn serve(config: Config, engine: EngineConfig) -> anyhow::Result<()> {
    let store = open_store(&config).await?;
    // The base URL the management agent calls the REST API with: an explicit
    // override, else derived from the bind address (loopback for a local daemon).
    let base_url = std::env::var("LAZYBONES_BASE_URL")
        .unwrap_or_else(|_| format!("http://{}", config.bind));
    // Asset bytes live in a content-addressed file blob store under the data dir
    // (swappable for S3/bucket behind the `BlobStore` trait). Shared between the
    // API (asset + extension `.wasm` uploads) and the scheduler's extension
    // component loader.
    let assets: Arc<dyn BlobStore> = Arc::new(FileBlobStore::new(config.data_dir.clone()));

    // EXTENSION RUNTIME (design §3.1/§3.4). Build the shared registry + Wasmtime
    // engine once and hand both halves to the API and the scheduler so an
    // install/enable/disable via REST is immediately visible to gate-check dispatch
    // and a circuit-breaker auto-disable is visible to the API. If the engine can't
    // be built (a bad Wasmtime config) the daemon still serves — it just runs
    // without extensions rather than refusing to boot.
    let registry = Arc::new(RwLock::new(Registry::new()));
    let ext_hooks = match ExtEngine::new(EngineLimits::default()) {
        Ok(ext_engine) => {
            let loader = Arc::new(BlobComponentLoader::new(assets.clone()));
            let dispatcher = Arc::new(Dispatcher::new(
                ext_engine.clone(),
                registry.clone(),
                loader,
                DispatcherConfig::default(),
            ));
            (Some(ext_engine), ExtHooks::new(dispatcher))
        }
        Err(e) => {
            tracing::warn!("extension engine init failed; serving without extensions: {e}");
            (None, ExtHooks::none())
        }
    };
    let (ext_engine, ext_hooks) = ext_hooks;

    let mut state = AppState::new(
        store.clone(),
        config.run.clone(),
        base_url,
        config.loop_token.clone(),
    )
    .with_assets(assets);
    if let Some(ext_engine) = ext_engine {
        state = state.with_ext_runtime(registry, ext_engine);
    }
    let app = router(state);

    // The loop is the daemon: if lazybonesd is up, the queue is being drained.
    // Wired with the extension hooks so gate-check runs at the gate point and the
    // event-reaction loop runs off the durable event stream.
    let sched = tokio::spawn(lazybones_engine::run_with_ext(store, engine, ext_hooks));

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!(bind = %config.bind, run = %config.run, "lazybonesd serving");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    sched.abort(); // stop the loop when the API stops
    Ok(())
}

/// Parse a workfile, resolve its specs, and sync the seeds into the store.
///
/// # Errors
/// Returns an error if the store cannot open or the workfile cannot be parsed.
pub async fn import(config: Config, workfile: &Path) -> anyhow::Result<()> {
    let store = open_store(&config).await?;
    let seeds = parse_workfile(workfile)?;
    let count = sync_seeds(&store, &config.run, &seeds).await?;
    tracing::info!(synced = count, run = %config.run, "workfile imported");
    Ok(())
}

/// Resolve when the process receives Ctrl-C, for a graceful shutdown.
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
