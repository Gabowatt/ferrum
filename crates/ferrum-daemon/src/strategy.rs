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

// ── Alpaca options snapshot types ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OptionsSnapshotResponse {
    snapshots: std::collections::HashMap<String, OptionSnapshot>,
}

#[derive(Debug, Deserialize)]
struct OptionSnapshot {
    #[serde(rename = "greeks")]
    greeks:   Option<Greeks>,
    #[serde(rename = "impliedVolatility")]
    iv:       Option<f64>,
    #[serde(rename = "latestQuote")]
    quote:    Option<OptionQuote>,
    #[serde(rename = "details")]
    details:  Option<ContractDetails>,
}

#[derive(Debug, Deserialize)]
struct Greeks {
    delta: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OptionQuote {
    #[serde(rename = "ap")]
    ask: Option<f64>,
    #[serde(rename = "bp")]
    bid: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ContractDetails {
    #[serde(rename = "expirationDate")]
    expiration_date: Option<String>,
    #[serde(rename = "strikePrice")]
    strike_price:    Option<f64>,
    #[serde(rename = "style")]
    style:           Option<String>,
}

// ── Delta scan strategy ───────────────────────────────────────────────────────

pub struct DeltaScanStrategy {
    symbols:     Vec<String>,
    delta_min:   f64,
    delta_max:   f64,
    dte_min:     u32,
    dte_max:     u32,
}

impl DeltaScanStrategy {
    pub fn new(config: &ferrum_core::config::AppConfig) -> Self {
        let dc = config.strategy.delta_scan.as_ref();
        Self {
            symbols:   config.strategy.symbols.clone(),
            delta_min: dc.map(|d| d.delta_min).unwrap_or(0.30),
            delta_max: dc.map(|d| d.delta_max).unwrap_or(0.50),
            dte_min:   dc.map(|d| d.dte_min).unwrap_or(7),
            dte_max:   dc.map(|d| d.dte_max).unwrap_or(45),
        }
    }
}

#[async_trait::async_trait]
impl Strategy for DeltaScanStrategy {
    fn name(&self) -> &str { "delta-scan" }

    async fn scan(&self, client: &AlpacaClient) -> Result<Vec<Signal>, FerrumError> {
        use chrono::Duration;

        let today     = chrono::Utc::now().date_naive();
        let exp_min   = (today + Duration::days(self.dte_min as i64)).to_string();
        let exp_max   = (today + Duration::days(self.dte_max as i64)).to_string();

        let mut signals = Vec::new();

        for symbol in &self.symbols {
            info!("[delta-scan] fetching options chain for {symbol}");

            let resp: OptionsSnapshotResponse = match client
                .get_with_query(
                    &format!("/v2/snapshots/options/{symbol}"),
                    &[
                        ("expiration_date_gte", exp_min.as_str()),
                        ("expiration_date_lte", exp_max.as_str()),
                        ("type", "call"),
                        ("limit", "250"),
                    ],
                )
                .await
            {
                Ok(r)  => r,
                Err(e) => {
                    return Err(FerrumError::Alpaca(format!(
                        "{symbol} options snapshot: {e}"
                    )));
                }
            };

            for (contract, snap) in &resp.snapshots {
                let delta = snap.greeks.as_ref().and_then(|g| g.delta).unwrap_or(0.0);
                if delta < self.delta_min || delta > self.delta_max {
                    continue;
                }

                let mid_price = match &snap.quote {
                    Some(q) => match (q.bid, q.ask) {
                        (Some(b), Some(a)) => (b + a) / 2.0,
                        _ => continue,
                    },
                    None => continue,
                };

                info!(
                    "[delta-scan] {contract}  delta={:.2}  iv={:.1}%  mid=${:.2}",
                    delta,
                    snap.iv.unwrap_or(0.0) * 100.0,
                    mid_price,
                );

                signals.push(Signal::EnterLong {
                    symbol: symbol.clone(),
                    legs: vec![ferrum_core::types::OptionLeg {
                        contract:    contract.clone(),
                        action:      ferrum_core::types::LegAction::Buy,
                        qty:         1,
                        order_type:  ferrum_core::types::OrderType::Limit,
                        limit_price: Some(mid_price),
                    }],
                });
            }
        }

        Ok(signals)
    }
}
