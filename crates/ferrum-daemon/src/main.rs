mod db;
mod exit_monitor;
mod ipc;
mod iv_rank;
mod order_poller;
mod orders;
mod pdt;
mod risk;
mod strategy;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::sync::{broadcast, Mutex, Notify};
use tracing::{error, info};

use ferrum_core::{
    client::AlpacaClient,
    config::{AppConfig, Mode},
    error::FerrumError,
    types::{BotStatus, LogEvent},
};
use crate::strategy::StrategyHandle;

/// In-memory metadata for a position we have opened.
#[derive(Debug, Clone)]
pub struct OpenPositionMeta {
    pub contract:              String,
    pub underlying:            String,
    pub direction:             String,
    pub opened_at:             chrono::DateTime<chrono::Utc>,
    pub entry_price:           f64,
    pub qty:                   u32,
    pub confluence_score:      u32,
    pub regime:                String,
    pub iv_rank:               f64,
    pub delta:                 f64,
    pub dte_at_entry:          u32,
    /// Order ID of the pending entry order (cleared once filled).
    pub pending_order_id:      Option<String>,
    /// Order ID of a pending close order (set when exit monitor submits close).
    pub pending_close_order_id: Option<String>,
    pub force_exit_next_open:  bool,
    /// Highest unrealized P&L % seen since entry — used for trailing profit target.
    /// Initialized to f64::NEG_INFINITY; updated each exit-monitor cycle.
    pub peak_pnl_pct:          f64,
}

#[derive(Debug)]
pub struct AppState {
    pub config:         AppConfig,
    pub client:         AlpacaClient,
    pub status:         Mutex<BotStatus>,
    pub log_tx:         broadcast::Sender<LogEvent>,
    pub db:             db::Database,
    pub pdt:            Mutex<pdt::PdtTracker>,
    pub open_positions: Mutex<std::collections::HashMap<String, OpenPositionMeta>>,
    /// Timestamp of the last position close for each underlying — used for cooldown veto.
    pub last_close_by_underlying: Mutex<std::collections::HashMap<String, chrono::DateTime<chrono::Utc>>>,
    /// Pinged when the user requests Stop so the strategy loop can interrupt its sleep.
    pub stop_notify: Arc<Notify>,
    /// V2.1 multi-strategy: every registered strategy gets one handle here,
    /// and IPC `Start` spawns one supervisor loop per handle. See
    /// `strategy::build_strategies` for what goes in this list.
    pub strategies: Vec<Arc<StrategyHandle>>,
    /// Live count of currently-running supervisor tasks. Used to coordinate
    /// the `Stopping → Idle` transition: the last loop to exit (counter == 0)
    /// flips the bot status, so we don't get a race between N loops each
    /// trying to set `Idle` independently.
    pub active_strategy_loops: AtomicUsize,
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

    // Live trading gate — require explicit opt-in via config
    if config.alpaca.mode == Mode::Live && !config.alpaca.live.enabled {
        error!("Live mode set but live.enabled = false in config. Set enabled = true to permit live trading.");
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

    // Build the strategy registry before AppState (handles need only the
    // config; they don't depend on AppState itself).
    let strategies = strategy::build_strategies(&config);
    info!(
        "Strategy registry: {} strategy(ies) registered ({})",
        strategies.len(),
        strategies.iter().map(|h| h.id).collect::<Vec<_>>().join(", "),
    );

    let state = Arc::new(AppState {
        config,
        client,
        status:         Mutex::new(BotStatus::Idle),
        log_tx:         log_tx.clone(),
        db,
        pdt:            Mutex::new(pdt_tracker),
        open_positions: Mutex::new(std::collections::HashMap::new()),
        last_close_by_underlying: Mutex::new(std::collections::HashMap::new()),
        stop_notify:    Arc::new(Notify::new()),
        strategies,
        active_strategy_loops: AtomicUsize::new(0),
    });

    // Persist log events to SQLite — subscribe before the first send.
    {
        let mut log_rx = log_tx.subscribe();
        let db = state.db.clone();
        tokio::spawn(async move {
            while let Ok(ev) = log_rx.recv().await {
                let ts = ev.timestamp.to_rfc3339();
                let lv = ev.level.to_string();
                let _ = db.insert_log(&ts, &lv, &ev.message).await;
            }
        });
    }

    let _ = log_tx.send(LogEvent::info("ferrum daemon started"));

    info!("Starting IPC server on /tmp/ferrum.sock");
    ipc::run_server(state.clone()).await?;

    Ok(())
}
