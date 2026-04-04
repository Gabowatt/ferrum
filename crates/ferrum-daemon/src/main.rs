mod db;
mod ipc;
mod risk;
mod strategy;

use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

use ferrum_core::{
    client::AlpacaClient,
    config::{AppConfig, Mode},
    error::FerrumError,
    types::{BotStatus, LogEvent},
};

/// Shared application state passed to IPC handlers and strategy tasks.
#[derive(Debug)]
pub struct AppState {
    pub config: AppConfig,
    pub client: AlpacaClient,
    pub status: Mutex<BotStatus>,
    pub log_tx: broadcast::Sender<LogEvent>,
    pub db:     db::Database,
}

#[tokio::main]
async fn main() -> Result<(), FerrumError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ferrum_daemon=info".parse().unwrap()),
        )
        .init();

    // ── Load config ──────────────────────────────────────────────────────────
    let cfg_path = std::env::var("FERRUM_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let config = AppConfig::load(&cfg_path)?;
    info!("Loaded config from {cfg_path}");

    // ── Live trading gate ────────────────────────────────────────────────────
    if config.alpaca.mode == Mode::Live {
        if !config.alpaca.live.enabled {
            error!("Live trading is disabled in V1. Set alpaca.live.enabled=true to unlock (not recommended).");
            return Err(FerrumError::LiveTradingDisabled);
        }
        error!("Live trading attempted — refusing in V1.");
        return Err(FerrumError::LiveTradingDisabled);
    }

    info!("Mode: {}", config.alpaca.mode);

    // ── Alpaca client ────────────────────────────────────────────────────────
    let client = AlpacaClient::new(&config)?;

    // ── Health check ─────────────────────────────────────────────────────────
    info!("Performing Alpaca health check...");
    let account: serde_json::Value = client.get("/v2/account").await
        .map_err(|e| {
            error!("Alpaca health check failed: {e}");
            e
        })?;
    info!(
        "Connected to Alpaca — account status: {}",
        account["status"].as_str().unwrap_or("unknown")
    );

    // ── SQLite ───────────────────────────────────────────────────────────────
    let db = db::Database::open().await?;
    db.migrate().await?;
    info!("Database ready");

    // ── Broadcast channel for log events (TUI subscribes via IPC) ───────────
    let (log_tx, _) = broadcast::channel::<LogEvent>(512);

    let state = Arc::new(AppState {
        config,
        client,
        status: Mutex::new(BotStatus::Idle),
        log_tx: log_tx.clone(),
        db,
    });

    // ── Emit startup log event ───────────────────────────────────────────────
    let _ = log_tx.send(LogEvent::info("ferrum daemon started"));

    // ── IPC server ───────────────────────────────────────────────────────────
    info!("Starting IPC server on /tmp/ferrum.sock");
    ipc::run_server(state.clone()).await?;

    Ok(())
}
