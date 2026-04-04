use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use serde::Deserialize;
use chrono::Utc;

use ferrum_core::{
    client::AlpacaClient,
    error::FerrumError,
    indicators::{self, TradeDirection},
    types::{BotStatus, FillRecord, LegAction, LogEvent, OptionLeg, OrderType, Signal},
};
use crate::{
    iv_rank::IvRankEngine,
    risk::RiskGuard,
    AppState,
};

// ── Strategy trait ────────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    async fn scan(&self, state: &AppState) -> Result<Vec<Signal>, FerrumError>;
}

// ── Main strategy loop ────────────────────────────────────────────────────────

pub async fn run_strategy_loop(state: Arc<AppState>) {
    let entry_interval = Duration::from_secs(state.config.strategy.scan_interval_secs);

    let strategy = IronConduitStrategy::new();

    loop {
        {
            let status = state.status.lock().await;
            if *status == BotStatus::Stopping {
                *state.status.lock().await = BotStatus::Idle;
                let _ = state.log_tx.send(LogEvent::info("strategy loop stopped"));
                return;
            }
        }

        let _ = state.log_tx.send(LogEvent::info("[iron-conduit] scan cycle starting"));

        match strategy.scan(&state).await {
            Ok(signals) if signals.is_empty() => {
                let _ = state.log_tx.send(LogEvent::info("[iron-conduit] no signals this cycle"));
            }
            Ok(signals) => {
                for signal in signals {
                    let guard = RiskGuard::new(&state.config, 0, 0.0, 1000.0, 0.0);
                    match guard.check_entry(&signal) {
                        Ok(()) => {
                            let _ = state.log_tx.send(LogEvent::risk("risk guard passed"));
                            let _ = state.log_tx.send(LogEvent::signal(format!("{signal:?}")));
                            // V1: log only — order submission wired in next milestone
                        }
                        Err(e) => {
                            let _ = state.log_tx.send(LogEvent::risk(format!("blocked: {e}")));
                        }
                    }
                }
            }
            Err(e) => {
                error!("[iron-conduit] scan error: {e}");
                let _ = state.log_tx.send(LogEvent::error(format!("scan error: {e}")));
            }
        }

        sleep(entry_interval).await;
    }
}

// ── Fill sync background task ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlpacaActivity {
    #[allow(dead_code)]
    id:               Option<String>,
    symbol:           Option<String>,
    side:             Option<String>,
    qty:              Option<String>,
    price:            Option<String>,
    transaction_time: Option<String>,
    order_id:         Option<String>,
}

pub async fn fill_sync_task(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        match sync_fills(&state).await {
            Ok(n) if n > 0 => {
                let _ = state.log_tx.send(LogEvent::info(format!("synced {n} new fills")));
            }
            Ok(_) => {}
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

// ── Alpaca market data types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct BarsResponse {
    bars: Vec<Bar>,
}

#[derive(Debug, Deserialize)]
struct Bar {
    #[serde(rename = "o")]  open:   f64,
    #[serde(rename = "h")]  high:   f64,
    #[serde(rename = "l")]  low:    f64,
    #[serde(rename = "c")]  close:  f64,
    #[serde(rename = "v")]  volume: f64,
    #[allow(dead_code)]
    #[serde(rename = "vw")] vwap:   Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OptionsSnapshotResponse {
    snapshots: std::collections::HashMap<String, OptionSnapshot>,
}

#[derive(Debug, Deserialize)]
struct OptionSnapshot {
    #[serde(rename = "greeks")]
    greeks: Option<Greeks>,
    #[serde(rename = "impliedVolatility")]
    iv:     Option<f64>,
    #[serde(rename = "latestQuote")]
    quote:  Option<OptionQuote>,
    #[serde(rename = "details")]
    details: Option<ContractDetails>,
}

#[derive(Debug, Deserialize)]
struct Greeks {
    delta: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OptionQuote {
    #[serde(rename = "ap")] ask: Option<f64>,
    #[serde(rename = "bp")] bid: Option<f64>,
    #[serde(rename = "as")] ask_size: Option<f64>,
    #[serde(rename = "bs")] bid_size: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ContractDetails {
    #[serde(rename = "expirationDate")]
    expiration_date: Option<String>,
    #[serde(rename = "openInterest")]
    open_interest:   Option<f64>,
    #[serde(rename = "type")]
    contract_type:   Option<String>,
}

// ── Iron Conduit Strategy ─────────────────────────────────────────────────────

pub struct IronConduitStrategy;

impl IronConduitStrategy {
    pub fn new() -> Self { Self }

    async fn fetch_bars(
        client: &AlpacaClient,
        symbol: &str,
        days: u32,
    ) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>), FerrumError> {
        let start = (Utc::now() - chrono::Duration::days(days as i64))
            .format("%Y-%m-%dT00:00:00Z").to_string();

        let resp: BarsResponse = client
            .get_with_query(
                &format!("/v2/stocks/{symbol}/bars"),
                &[
                    ("timeframe", "1Day"),
                    ("start", &start),
                    ("limit", "300"),
                    ("feed", "iex"),
                ],
            )
            .await
            .map_err(|e| FerrumError::Alpaca(format!("{symbol} bars: {e}")))?;

        let closes:  Vec<f64> = resp.bars.iter().map(|b| b.close).collect();
        let highs:   Vec<f64> = resp.bars.iter().map(|b| b.high).collect();
        let lows:    Vec<f64> = resp.bars.iter().map(|b| b.low).collect();
        let volumes: Vec<f64> = resp.bars.iter().map(|b| b.volume).collect();
        Ok((closes, highs, lows, volumes))
    }
}

#[async_trait::async_trait]
impl Strategy for IronConduitStrategy {
    fn name(&self) -> &str { "iron-conduit" }

    async fn scan(&self, state: &AppState) -> Result<Vec<Signal>, FerrumError> {
        let cfg      = &state.config;
        let rc       = &cfg.strategy.regime;
        let entry    = &cfg.strategy.entry;
        let liq      = &cfg.liquidity;
        let iv_cfg   = &cfg.iv_engine;

        let iv_engine = IvRankEngine::new(
            iv_cfg.iv_rank_buy_max,
            iv_cfg.iv_rank_caution_min,
            iv_cfg.iv_rank_caution_factor,
            iv_cfg.hv_lookback_days,
        );

        let today    = Utc::now().date_naive();
        let exp_min  = (today + chrono::Duration::days(entry.dte_min as i64)).to_string();
        let exp_max  = (today + chrono::Duration::days(entry.dte_max as i64)).to_string();

        let mut signals = Vec::new();

        // Collect all symbols, respecting tier3 IV rank gate
        let all_symbols: Vec<&str> = cfg.symbols.all();

        for symbol in all_symbols {
            // Fetch daily bars for indicator computation
            let (closes, highs, lows, volumes) =
                match Self::fetch_bars(&state.client, symbol, 90).await {
                    Ok(data) => data,
                    Err(e) => {
                        warn!("[iron-conduit] {symbol} bars fetch failed: {e}");
                        continue;
                    }
                };

            if closes.len() < 60 {
                info!("[iron-conduit] {symbol}: insufficient bar history ({} bars)", closes.len());
                continue;
            }

            // Compute indicators
            let snap = match indicators::compute_snapshot(
                &closes, &highs, &lows, &volumes,
                rc.adx_trend_threshold, rc.adx_no_trend_threshold,
            ) {
                Some(s) => s,
                None => { info!("[iron-conduit] {symbol}: snapshot failed"); continue; }
            };

            info!("[iron-conduit] {symbol}: regime={} ema9={:.2} ema20={:.2} rsi={:.1} adx={:.1}",
                snap.regime, snap.ema9, snap.ema20, snap.rsi, snap.adx.adx);

            // Confluence gate
            let (score, direction) = match indicators::confluence_score(
                &snap, rc.rsi_overbought, rc.rsi_oversold,
            ) {
                Some(s) => s,
                None => {
                    info!("[iron-conduit] {symbol}: choppy regime — skipping");
                    continue;
                }
            };

            info!("[iron-conduit] {symbol}: confluence score={score} direction={direction:?}");

            if score < entry.min_confluence_score {
                info!("[iron-conduit] {symbol}: score {score} < min {}", entry.min_confluence_score);
                continue;
            }

            // IV rank check
            let contract_type_str = match direction {
                TradeDirection::Call => "call",
                TradeDirection::Put  => "put",
            };

            // Fetch options chain
            let chain_resp: OptionsSnapshotResponse = match state.client
                .get_with_query(
                    &format!("/v2/snapshots/options/{symbol}"),
                    &[
                        ("expiration_date_gte", exp_min.as_str()),
                        ("expiration_date_lte", exp_max.as_str()),
                        ("type", contract_type_str),
                        ("limit", "250"),
                    ],
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!("[iron-conduit] {symbol} chain fetch failed: {e}");
                    continue;
                }
            };

            // Collect and rank qualifying contracts
            let mut candidates: Vec<(&String, &OptionSnapshot, f64, f64, f64)> = Vec::new();

            for (contract, snap_opt) in &chain_resp.snapshots {
                let delta = snap_opt.greeks.as_ref().and_then(|g| g.delta).unwrap_or(0.0);
                let delta_abs = delta.abs();

                if delta_abs < entry.delta_min || delta_abs > entry.delta_max { continue; }

                // Liquidity checks
                let oi = snap_opt.details.as_ref().and_then(|d| d.open_interest).unwrap_or(0.0);
                if oi < liq.min_open_interest as f64 { continue; }

                let (bid, ask) = match &snap_opt.quote {
                    Some(q) => match (q.bid, q.ask) {
                        (Some(b), Some(a)) => (b, a),
                        _ => continue,
                    },
                    None => continue,
                };

                if ask - bid > liq.max_bid_ask_spread { continue; }
                if ask <= 0.0 { continue; }

                // Premium budget check
                let mid = (bid + ask) / 2.0;
                if mid * 100.0 > cfg.sizing.max_position_usd { continue; }

                // IV rank check
                let current_iv = snap_opt.iv.unwrap_or(0.0);
                let iv_result = iv_engine
                    .compute(symbol, current_iv, &closes, &state.db)
                    .await
                    .unwrap_or_else(|_| crate::iv_rank::IvRankResult {
                        iv_rank: 50.0, current_iv, method: crate::iv_rank::IvMethod::HvProxy,
                    });

                // Tier 3 symbols require elevated IV rank
                if cfg.symbols.tier_of(symbol) == Some(3)
                    && iv_result.iv_rank < cfg.symbols.tier3_iv_rank_min
                {
                    continue;
                }

                if !iv_engine.is_buyable(iv_result.iv_rank) {
                    info!("[iron-conduit] {symbol} {contract}: IV rank {:.1} too high — skip",
                        iv_result.iv_rank);
                    continue;
                }

                // Store IV snapshot
                let _ = state.db.upsert_iv_snapshot(symbol, current_iv, snap.hv20, iv_result.iv_rank).await;

                // Score contract: prefer delta closest to preferred, tightest spread, highest OI
                let delta_score = (delta_abs - entry.preferred_delta).abs();
                candidates.push((contract, snap_opt, mid, delta_score, oi));
            }

            // Rank: lowest delta_score first, then highest OI
            candidates.sort_by(|a, b| {
                a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal))
            });

            if let Some((contract, _, mid, delta_score, oi)) = candidates.first() {
                let size_factor  = cfg.sizing.size_factor_for(score);
                let iv_adj       = iv_engine.size_factor(0.0); // default 1.0 — refined per contract above
                let qty          = ((cfg.sizing.max_position_usd * size_factor * iv_adj)
                    / (mid * 100.0))
                    .floor() as u32;
                let qty = qty.max(1);

                let action = match direction {
                    TradeDirection::Call | TradeDirection::Put => LegAction::Buy,
                };

                info!(
                    "[iron-conduit] SIGNAL {symbol} {contract} \
                     dir={direction:?} mid=${mid:.2} score={score} \
                     delta_dist={delta_score:.3} oi={oi:.0} qty={qty}"
                );

                signals.push(Signal::EnterLong {
                    symbol: symbol.to_string(),
                    legs: vec![OptionLeg {
                        contract:    (*contract).clone(),
                        action,
                        qty,
                        order_type:  OrderType::Limit,
                        limit_price: Some(*mid),
                    }],
                });
            }
        }

        Ok(signals)
    }
}
