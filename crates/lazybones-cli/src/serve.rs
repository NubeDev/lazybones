//! Open the store and either serve the REST API or run a one-shot import.

use std::path::Path;

use lazybones_api::{AppState, router};
use lazybones_store::{StoreEngine, StoreHandle, sync_seeds};

use crate::configure::Config;
use crate::workfile::parse_workfile;

/// Open the embedded store at the configured data dir, namespace, and database.
async fn open_store(config: &Config) -> anyhow::Result<StoreHandle> {
    let engine = StoreEngine::File {
        path: config.data_dir.clone(),
    };
    let store = StoreHandle::open(&engine, &config.namespace, &config.database).await?;
    Ok(store)
}

/// Serve the REST API until the process is signalled.
///
/// # Errors
/// Returns an error if the store cannot open or the listener cannot bind.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let store = open_store(&config).await?;
    let state = AppState::new(store, config.run.clone(), config.loop_token.clone());
    let app = router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!(bind = %config.bind, run = %config.run, "lazybonesd serving");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
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
