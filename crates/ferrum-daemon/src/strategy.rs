use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use serde::Deserialize;
use chrono::Utc;

use ferrum_core::{
    client::AlpacaClient,
    types::{BotStatus, FillRecord, LogEvent, Signal},
    error::FerrumError,
};
use crate::{risk::RiskGuard, AppState};

// ── Strategy trait ────────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    async fn scan(&self, client: &AlpacaClient) -> Result<Vec<Signal>, FerrumError>;
}

// ── Main strategy loop ────────────────────────────────────────────────────────

pub async fn run_strategy_loop(state: Arc<AppState>) {
    let interval = Duration::from_secs(state.config.strategy.scan_interval_secs);
    let strategies: Vec<Box<dyn Strategy>> = vec![
        Box::new(DeltaScanStrategy::new(&state.config)),
    ];

    loop {
        // Check if we should stop.
        {
            let status = state.status.lock().await;
            if *status == BotStatus::Stopping {
                *state.status.lock().await = BotStatus::Idle; // reset after stopping
                let _ = state.log_tx.send(LogEvent::info("strategy loop stopped"));
                return;
            }
        }

        for strat in &strategies {
            let _ = state.log_tx.send(LogEvent::info(
                format!("[{}] scanning...", strat.name())
            ));

            match strat.scan(&state.client).await {
                Ok(signals) if signals.is_empty() => {
                    let _ = state.log_tx.send(LogEvent::info(
                        format!("[{}] no signals", strat.name())
                    ));
                }
                Ok(signals) => {
                    for signal in signals {
                        let guard = RiskGuard::new(&state.config, 0, 0.0);
                        match guard.check_signal(&signal) {
                            Ok(()) => {
                                let _ = state.log_tx.send(LogEvent::risk("risk guard passed"));
                                let _ = state.log_tx.send(LogEvent::signal(
                                    format!("signal: {:?}", signal)
                                ));
                                // V1: log only, no order submission
                            }
                            Err(e) => {
                                let _ = state.log_tx.send(LogEvent::risk(
                                    format!("risk violation: {e}")
                                ));
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("[{}] scan error: {e}", strat.name());
                    let _ = state.log_tx.send(LogEvent::error(
                        format!("[{}] scan error: {e}", strat.name())
                    ));
                }
            }
        }

        sleep(interval).await;
    }
}

// ── Fill sync background task ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlpacaActivity {
    id:           Option<String>,
    symbol:       Option<String>,
    side:         Option<String>,
    qty:          Option<String>,
    price:        Option<String>,
    transaction_time: Option<String>,
    order_id:     Option<String>,
}

pub async fn fill_sync_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        match sync_fills(&state).await {
            Ok(n) => {
                if n > 0 {
                    let _ = state.log_tx.send(LogEvent::info(format!("synced {n} new fills")));
                }
            }
            Err(e) => {
                let _ = state.log_tx.send(LogEvent::error(format!("fill sync error: {e}")));
            }
        }
    }
}

async fn sync_fills(state: &AppState) -> Result<usize, FerrumError> {
    let activities: Vec<AlpacaActivity> = state.client
        .get_with_query("/v2/account/activities", &[("activity_types", "FILL")])
        .await?;

    let mut count = 0;
    for act in activities {
        let Some(order_id) = act.order_id else { continue };
        let fill = FillRecord {
            id:        None,
            symbol:    act.symbol.unwrap_or_default(),
            side:      act.side.unwrap_or_default(),
            qty:       act.qty.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            price:     act.price.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            timestamp: act.transaction_time
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(Utc::now),
            order_id,
        };
        state.db.upsert_fill(&fill).await?;
        count += 1;
    }
    Ok(count)
}

// ── Delta scan strategy (stub) ────────────────────────────────────────────────

pub struct DeltaScanStrategy {
    symbols:     Vec<String>,
    delta_min:   f64,
    delta_max:   f64,
    iv_rank_min: f64,
    dte_min:     u32,
    dte_max:     u32,
}

impl DeltaScanStrategy {
    pub fn new(config: &ferrum_core::config::AppConfig) -> Self {
        let dc = config.strategy.delta_scan.as_ref();
        Self {
            symbols:     config.strategy.symbols.clone(),
            delta_min:   dc.map(|d| d.delta_min).unwrap_or(0.30),
            delta_max:   dc.map(|d| d.delta_max).unwrap_or(0.50),
            iv_rank_min: dc.map(|d| d.iv_rank_min).unwrap_or(40.0),
            dte_min:     dc.map(|d| d.dte_min).unwrap_or(7),
            dte_max:     dc.map(|d| d.dte_max).unwrap_or(45),
        }
    }
}

#[async_trait::async_trait]
impl Strategy for DeltaScanStrategy {
    fn name(&self) -> &str { "delta-scan" }

    async fn scan(&self, client: &AlpacaClient) -> Result<Vec<Signal>, FerrumError> {
        let signals: Vec<Signal> = Vec::new();

        for symbol in &self.symbols {
            info!("Scanning {symbol} options chain...");

            // Placeholder: in Milestone 2.3+ this calls Polygon for options chain data.
            // For now we log and return no signals until Polygon integration is wired up.
            let _ = (client, self.delta_min, self.delta_max, self.iv_rank_min, self.dte_min, self.dte_max);
            info!("  {symbol}: Polygon options chain fetch not yet wired (stub)");
        }

        Ok(signals)
    }
}
