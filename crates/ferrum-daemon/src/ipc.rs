use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    signal,
};
use tracing::{error, info};
use serde::Deserialize;

use ferrum_core::types::{BotStatus, IpcCommand, IpcResponse, LogEvent, Position, StrategyInfo};

use crate::{exit_monitor, strategy, AppState};

const SOCK_PATH: &str = "/tmp/ferrum.sock";

pub async fn run_server(state: Arc<AppState>) -> Result<(), ferrum_core::error::FerrumError> {
    // Remove stale socket if it exists.
    let _ = std::fs::remove_file(SOCK_PATH);

    let listener = UnixListener::bind(SOCK_PATH)?;
    info!("IPC listening on {SOCK_PATH}");

    // Spawn background tasks.
    tokio::spawn(crate::strategy::fill_sync_task(state.clone()));
    tokio::spawn(exit_monitor::run_exit_monitor(state.clone()));
    tokio::spawn(crate::order_poller::run_order_poller(state.clone()));

    // Graceful shutdown via SIGINT / SIGTERM.
    let state_shutdown = state.clone();
    tokio::spawn(async move {
        let mut sigint  = signal::unix::signal(signal::unix::SignalKind::interrupt()).unwrap();
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate()).unwrap();
        tokio::select! {
            _ = sigint.recv()  => {},
            _ = sigterm.recv() => {},
        }
        info!("Shutdown signal received — stopping");
        *state_shutdown.status.lock().await = BotStatus::Stopping;
        let _ = state_shutdown.log_tx.send(LogEvent::warn("daemon shutting down"));
        std::fs::remove_file(SOCK_PATH).ok();
        std::process::exit(0);
    });

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let state = state.clone();
                tokio::spawn(handle_connection(stream, state));
            }
            Err(e) => {
                error!("IPC accept error: {e}");
            }
        }
    }
}

async fn handle_connection(stream: UnixStream, state: Arc<AppState>) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let cmd: IpcCommand = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(e) => {
                let resp = IpcResponse::Error { message: format!("bad command: {e}") };
                let _ = writer.write_all(json_line(&resp).as_bytes()).await;
                continue;
            }
        };

        let response = dispatch(cmd, &state).await;
        let _ = writer.write_all(json_line(&response).as_bytes()).await;
    }
}

async fn dispatch(cmd: IpcCommand, state: &Arc<AppState>) -> IpcResponse {
    match cmd {
        IpcCommand::Status => {
            let status = *state.status.lock().await;
            IpcResponse::Status {
                status,
                mode: state.config.alpaca.mode.to_string(),
            }
        }

        IpcCommand::Start => {
            let mut s = state.status.lock().await;
            if *s == BotStatus::Running {
                return IpcResponse::Error { message: "already running".into() };
            }
            *s = BotStatus::Running;
            drop(s);
            // V2.1: spawn one supervisor task per registered strategy handle.
            // Each loop is responsible for its own scan cadence + enable check;
            // they all observe the global Stopping flag via `stop_notify`.
            let ids: Vec<&'static str> = state.strategies.iter().map(|h| h.id).collect();
            let _ = state.log_tx.send(LogEvent::info(format!(
                "strategy loops starting: {}", ids.join(", ")
            )));
            for handle in &state.strategies {
                tokio::spawn(strategy::run_strategy_loop(handle.clone(), state.clone()));
            }
            IpcResponse::Ok
        }

        IpcCommand::Stop => {
            let mut s = state.status.lock().await;
            if *s != BotStatus::Running {
                return IpcResponse::Error { message: "not running".into() };
            }
            *s = BotStatus::Stopping;
            drop(s);
            state.stop_notify.notify_waiters();
            let _ = state.log_tx.send(LogEvent::warn("strategy loop stopping"));
            IpcResponse::Ok
        }

        IpcCommand::ToggleMode { mode } => {
            let cfg_path = std::env::var("FERRUM_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
            match toggle_mode_in_config(&cfg_path, &mode) {
                Ok(()) => {
                    let _ = state.log_tx.send(LogEvent::warn(format!(
                        "mode set to {mode} in {cfg_path} — restart daemon to apply"
                    )));
                    IpcResponse::Ok
                }
                Err(e) => IpcResponse::Error { message: format!("config write failed: {e}") },
            }
        }

        IpcCommand::GetPnl { period } => {
            match fetch_pnl(&state.client, &period).await {
                Ok(resp) => resp,
                Err(e)   => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcCommand::GetFills => {
            match state.db.recent_fills(50).await {
                Ok(fills) => IpcResponse::Fills { fills },
                Err(e)    => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcCommand::GetPositions => {
            match fetch_positions(state).await {
                Ok(positions) => IpcResponse::Positions { positions },
                Err(e)        => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcCommand::GetPdt => {
            let pdt = state.pdt.lock().await;
            IpcResponse::PdtStatus {
                used: pdt.count_in_window(),
                max:  pdt.max_per_window,
            }
        }

        IpcCommand::GetMarketClock => {
            match fetch_market_clock(&state.client).await {
                Ok(resp) => resp,
                Err(e)   => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcCommand::GetLogs { limit } => {
            match state.db.recent_logs(limit as i64).await {
                Ok(events) => IpcResponse::Logs { events },
                Err(e)     => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcCommand::GetEquityHistory { period } => {
            match fetch_equity_history(&state.client, &period).await {
                Ok(resp) => resp,
                Err(e)   => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcCommand::GetStrategies => {
            // Snapshot per-strategy stats: position count from the in-memory map
            // and the today scan tally from SQLite. Both are fast enough to do
            // per request — no need to cache for now.
            let positions_by_strategy: std::collections::HashMap<&'static str, u32> = {
                let positions = state.open_positions.lock().await;
                let mut map: std::collections::HashMap<&'static str, u32> = std::collections::HashMap::new();
                for meta in positions.values() {
                    *map.entry(meta.strategy_id).or_insert(0) += 1;
                }
                map
            };

            let mut infos: Vec<StrategyInfo> = Vec::with_capacity(state.strategies.len());
            for handle in &state.strategies {
                let (scans_today, signals_today) = state.db.scan_tally_today(handle.id).await
                    .unwrap_or((0, 0));
                infos.push(StrategyInfo {
                    id:                 handle.id.to_string(),
                    enabled:            handle.is_enabled(),
                    scan_interval_secs: handle.scan_interval.as_secs(),
                    open_positions:     positions_by_strategy.get(handle.id).copied().unwrap_or(0),
                    signals_today,
                    scans_today,
                });
            }
            IpcResponse::Strategies { strategies: infos }
        }

        IpcCommand::SetStrategyEnabled { id, enabled } => {
            // 1. Find handle by id; flip the AtomicBool live.
            let handle = state.strategies.iter().find(|h| h.id == id);
            let Some(handle) = handle else {
                return IpcResponse::Error { message: format!("unknown strategy id: {id}") };
            };
            handle.set_enabled(enabled);

            // 2. Persist to config.toml so the choice survives a daemon restart.
            //    Loss of the live flag here would only mean the AtomicBool is
            //    correct in this process; logging it loud + returning Error so
            //    the UI surfaces the persistence failure to the operator.
            let cfg_path = std::env::var("FERRUM_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
            match set_strategy_enabled_in_config(&cfg_path, &id, enabled) {
                Ok(()) => {
                    let state_str = if enabled { "enabled" } else { "disabled" };
                    let _ = state.log_tx.send(LogEvent::info(format!(
                        "strategy '{id}' {state_str} (live + persisted to {cfg_path})"
                    )));
                    IpcResponse::Ok
                }
                Err(e) => {
                    let _ = state.log_tx.send(LogEvent::error(format!(
                        "strategy '{id}' live-toggled but config write failed: {e}"
                    )));
                    IpcResponse::Error { message: format!("config write failed: {e}") }
                }
            }
        }
    }
}

// ── Alpaca position helper ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlpacaPositionRaw {
    symbol:          String,
    qty:             String,
    unrealized_pl:   String,
    unrealized_plpc: String,
    current_price:   String,
    cost_basis:      String,
}

async fn fetch_positions(state: &AppState) -> Result<Vec<Position>, ferrum_core::error::FerrumError> {
    let raw: Vec<AlpacaPositionRaw> = state.client.get("/v2/positions").await?;

    let open = state.open_positions.lock().await;
    let positions = raw.into_iter().map(|ap| {
        let qty:             f64 = ap.qty.parse().unwrap_or(0.0);
        let unrealized_pl:   f64 = ap.unrealized_pl.parse().unwrap_or(0.0);
        let unrealized_plpc: f64 = ap.unrealized_plpc.parse().unwrap_or(0.0);
        let current_price:   f64 = ap.current_price.parse().unwrap_or(0.0);
        let cost_basis:      f64 = ap.cost_basis.parse().unwrap_or(0.0);
        let entry_price = if qty != 0.0 { cost_basis / (qty * 100.0) } else { 0.0 };

        let (underlying, direction, opened_at, strategy_id) = match open.get(&ap.symbol) {
            Some(meta) => (
                meta.underlying.clone(),
                meta.direction.clone(),
                meta.opened_at,
                Some(meta.strategy_id.to_string()),
            ),
            None => (
                ap.symbol.clone(),
                if ap.symbol.contains('C') { "call".to_string() } else { "put".to_string() },
                chrono::Utc::now(),
                None,  // legacy / manual position with no strategy attribution
            ),
        };

        Position {
            contract:        ap.symbol,
            underlying,
            direction,
            qty,
            entry_price,
            current_price,
            market_value:    current_price * qty * 100.0,
            unrealized_pl,
            unrealized_plpc,
            opened_at,
            strategy_id,
        }
    }).collect();

    Ok(positions)
}

async fn fetch_pnl(
    client: &ferrum_core::client::AlpacaClient,
    _period: &str,
) -> Result<IpcResponse, ferrum_core::error::FerrumError> {
    // One month of daily history: equity[-1] - equity[-2] = today, profit_loss[-1] = month total.
    let month_data: serde_json::Value = client
        .get_with_query("/v2/account/portfolio/history", &[("period", "1M"), ("timeframe", "1D")])
        .await?;

    let equity_arr = month_data["equity"].as_array();
    let pl_arr     = month_data["profit_loss"].as_array();

    // Today = last day's equity minus previous day's equity.
    let today = equity_arr.and_then(|arr| {
        let n = arr.len();
        if n >= 2 {
            let last = arr[n - 1].as_f64()?;
            let prev = arr[n - 2].as_f64()?;
            Some(last - prev)
        } else {
            None
        }
    }).unwrap_or(0.0);

    // Month = cumulative profit_loss since start of the 1M window.
    let month = pl_arr
        .and_then(|arr| arr.last())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // Year = cumulative profit_loss over 1A window (best-effort, falls back to 0).
    let year = match client
        .get_with_query::<serde_json::Value>(
            "/v2/account/portfolio/history",
            &[("period", "1A"), ("timeframe", "1D")],
        )
        .await
    {
        Ok(year_data) => year_data["profit_loss"].as_array()
            .and_then(|arr| arr.last())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        Err(_) => 0.0,
    };

    Ok(IpcResponse::Pnl { today, month, year })
}

// ── Market clock helper ────────────────────────────────────────────────────────

async fn fetch_market_clock(
    client: &ferrum_core::client::AlpacaClient,
) -> Result<IpcResponse, ferrum_core::error::FerrumError> {
    #[derive(serde::Deserialize)]
    struct Clock {
        is_open:    bool,
        next_open:  String,
        next_close: String,
    }

    let clock: Clock = client.get("/v2/clock").await?;

    let next_change = if clock.is_open {
        // Show local time of next close
        parse_clock_time(&clock.next_close)
            .map(|t| format!("closes {t}"))
            .unwrap_or_else(|| "closes --:--".to_string())
    } else {
        parse_clock_time(&clock.next_open)
            .map(|t| format!("opens {t}"))
            .unwrap_or_else(|| "opens --:--".to_string())
    };

    Ok(IpcResponse::MarketClock { is_open: clock.is_open, next_change })
}

/// Parse an RFC3339 timestamp and return "HH:MM" in local time.
fn parse_clock_time(ts: &str) -> Option<String> {
    use chrono::{DateTime, Local};
    let dt = DateTime::parse_from_rfc3339(ts).ok()?;
    Some(dt.with_timezone(&Local).format("%H:%M").to_string())
}

async fn fetch_equity_history(
    client: &ferrum_core::client::AlpacaClient,
    period: &str,
) -> Result<IpcResponse, ferrum_core::error::FerrumError> {
    let data: serde_json::Value = client
        .get_with_query(
            "/v2/account/portfolio/history",
            &[("period", period), ("timeframe", "1D")],
        )
        .await?;

    let timestamps: Vec<i64> = data["timestamp"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_i64().map(|t| t * 1000)).collect())
        .unwrap_or_default();

    let equity: Vec<f64> = data["equity"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();

    Ok(IpcResponse::EquityHistory { timestamps, equity })
}

/// Persist `[strategies.<id>].enabled = <bool>` to config.toml, preserving
/// comments + ordering. Creates the section/key if missing.
///
/// Uses `toml_edit` (vs. the line-based approach `toggle_mode_in_config` uses
/// for the top-level `mode` key) because we may need to insert a brand-new
/// nested table — line splicing for that is fragile.
fn set_strategy_enabled_in_config(path: &str, id: &str, enabled: bool) -> Result<(), String> {
    use toml_edit::{value, DocumentMut, Item, Table};

    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("read {path}: {e}"))?;
    let mut doc: DocumentMut = raw.parse()
        .map_err(|e| format!("parse {path}: {e}"))?;

    // Ensure top-level `[strategies]` table exists. Using `or_insert` with a
    // freshly-constructed table preserves anything the user might have already
    // put there.
    let strategies = doc
        .as_table_mut()
        .entry("strategies")
        .or_insert(Item::Table(Table::new()));

    let Item::Table(strategies_tbl) = strategies else {
        return Err("[strategies] is present but not a table".into());
    };
    // Mark as implicit so the resulting TOML stays as `[strategies.<id>]`
    // (subtable headers) instead of becoming an inline `strategies = { ... }`.
    strategies_tbl.set_implicit(true);

    let entry = strategies_tbl
        .entry(id)
        .or_insert(Item::Table(Table::new()));

    let Item::Table(entry_tbl) = entry else {
        return Err(format!("[strategies.{id}] is present but not a table"));
    };
    entry_tbl["enabled"] = value(enabled);

    std::fs::write(path, doc.to_string())
        .map_err(|e| format!("write {path}: {e}"))
}

fn toggle_mode_in_config(path: &str, mode: &str) -> std::io::Result<()> {
    let content = std::fs::read_to_string(path)?;
    let updated = content
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("mode") && line.contains('=') {
                format!("mode = \"{}\"", mode)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(path, updated + "\n")
}

fn json_line(resp: &IpcResponse) -> String {
    let mut s = serde_json::to_string(resp).unwrap_or_else(|_| "{}".to_string());
    s.push('\n');
    s
}
