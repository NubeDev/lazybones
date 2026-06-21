//! `lazybonesd` — parse config, open the embedded store, serve the REST API.
//!
//! Two subcommands:
//!   `lazybonesd serve`            — open the store and serve (default).
//!   `lazybonesd import <wf.yaml>` — parse a workfile, resolve specs, sync into
//!                                   the store (the seed → DB import), then exit.
//!
//! Boot config comes from `lazybones.yaml` + `LAZYBONES_*` env (see `configure`).

mod configure;
mod serve;
mod workfile;

use std::path::PathBuf;

use configure::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            // Default to info across all our crates — the engine especially, whose
            // scheduler decisions (promote/claim/block/auto-retry) are the main
            // operational signal. It was omitted here, so those lines were silently
            // filtered out by default; only an explicit RUST_LOG showed them.
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "lazybones_cli=info,lazybones_api=info,lazybones_engine=info".into()
            }),
        )
        .init();

    let config_path = config_path();
    let config = Config::load(&config_path)?;

    match command() {
        Command::Serve => {
            // The scheduler keys (gate, concurrency, worktrees, …) load from the
            // same file + LAZYBONES_* env; only `serve` runs the loop.
            let engine = lazybones_engine::EngineConfig::load(&config_path)?;
            serve::serve(config, engine).await
        }
        Command::Import(path) => serve::import(config, &path).await,
    }
}

/// The parsed subcommand.
enum Command {
    Serve,
    Import(PathBuf),
}

/// Parse `argv` into a [`Command`], defaulting to `serve`.
fn command() -> Command {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("import") => {
            let path = args
                .next()
                .map_or_else(|| PathBuf::from("lazybones/workfile.yaml"), PathBuf::from);
            Command::Import(path)
        }
        _ => Command::Serve,
    }
}

/// Where to look for `lazybones.yaml` (override with `LAZYBONES_CONFIG`).
fn config_path() -> PathBuf {
    std::env::var("LAZYBONES_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("lazybones/lazybones.yaml"))
}
