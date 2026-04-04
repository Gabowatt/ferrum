mod db;
mod ipc;
mod iv_rank;
mod pdt;
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

#[derive(Debug)]
pub struct AppState {
    pub config:  AppConfig,
    pub client:  AlpacaClient,
    pub status:  Mutex<BotStatus>,
    pub log_tx:  broadcast::Sender<LogEvent>,
    pub db:      db::Database,
    pub pdt:     Mutex<pdt::PdtTracker>,
}

#[tokio::main]
async fn main() -> Result<(), FerrumError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ferrum_daemon=info".parse().unwrap()),
        )
        .init();

    let cfg_path = std::env::var("FERRUM_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    let config = AppConfig::load(&cfg_path)?;
    info!("Loaded config from {cfg_path}");

    // Live trading gate
    if config.alpaca.mode == Mode::Live {
        if !config.alpaca.live.enabled {
            error!("Live trading is disabled in V1.");
            return Err(FerrumError::LiveTradingDisabled);
        }
        error!("Live trading attempted — refusing in V1.");
        return Err(FerrumError::LiveTradingDisabled);
    }
    info!("Mode: {}", config.alpaca.mode);

    let client = AlpacaClient::new(&config)?;

    // Health check
    info!("Performing Alpaca health check...");
    let account: serde_json::Value = client.get("/v2/account").await
        .map_err(|e| { error!("Alpaca health check failed: {e}"); e })?;
    info!("Connected — account status: {}", account["status"].as_str().unwrap_or("unknown"));

    // SQLite
    let db = db::Database::open().await?;
    db.migrate().await?;
    info!("Database ready");

    // PDT tracker — load history from DB
    let mut pdt_tracker = pdt::PdtTracker::new(
        config.pdt.max_day_trades_per_5d,
        config.pdt.rolling_window_days,
        config.pdt.emergency_stop_pct,
        config.pdt.exceptional_win_pct,
    );
    pdt_tracker.load_from_db(&db).await?;
    let dt_count = pdt_tracker.count_in_window();
    info!("PDT tracker loaded — {dt_count}/{} day trades in current window",
        config.pdt.max_day_trades_per_5d);

    let (log_tx, _) = broadcast::channel::<LogEvent>(512);

    let state = Arc::new(AppState {
        config,
        client,
        status: Mutex::new(BotStatus::Idle),
        log_tx: log_tx.clone(),
        db,
        pdt: Mutex::new(pdt_tracker),
    });

    let _ = log_tx.send(LogEvent::info("ferrum daemon started"));

    // Fill sync background task
    tokio::spawn(strategy::fill_sync_task(state.clone()));

    info!("Starting IPC server on /tmp/ferrum.sock");
    ipc::run_server(state.clone()).await?;

    Ok(())
}
