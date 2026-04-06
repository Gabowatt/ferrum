use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use serde::Deserialize;
use chrono::{Datelike, Timelike, Utc};

use ferrum_core::{
    client::AlpacaClient,
    error::FerrumError,
    indicators::{self, TradeDirection},
    types::{BotStatus, FillRecord, LegAction, LogEvent, OptionLeg, OrderType, Signal},
};
use crate::{
    iv_rank::IvRankEngine,
    orders,
    risk::RiskGuard,
    AppState, OpenPositionMeta,
};

// ── Strategy trait ────────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    async fn scan(&self, state: &AppState) -> Result<Vec<Signal>, FerrumError>;
}

// ── Alpaca clock ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlpacaClock {
    is_open:    bool,
    next_open:  Option<String>,
    next_close: Option<String>,
}

/// Returns true if the market is open AND the current ET time is within the
/// configured scan window.
async fn market_is_open(state: &AppState) -> bool {
    let clock: AlpacaClock = match state.client.get("/v2/clock").await {
        Ok(c)  => c,
        Err(e) => {
            let _ = state.log_tx.send(LogEvent::warn(format!("clock fetch failed: {e}")));
            return false;
        }
    };

    if !clock.is_open {
        let _ = state.log_tx.send(LogEvent::info(format!(
            "market closed — skipping scan (next open: {})",
            clock.next_open.as_deref().unwrap_or("unknown")
        )));
        return false;
    }

    // Check configured ET scan window using UTC time offset.
    // EDT (UTC-4) Mar–Nov, EST (UTC-5) Nov–Mar.
    let now_utc = Utc::now();
    let month = now_utc.month();
    let et_offset_hours: i64 = if month >= 3 && month <= 11 { -4 } else { -5 };
    let et_hour = (now_utc.hour() as i64 + et_offset_hours).rem_euclid(24) as u32;
    let et_min  = now_utc.minute();
    let et_mins = et_hour * 60 + et_min;

    fn parse_hhmm(s: &str) -> Option<u32> {
        let mut parts = s.splitn(2, ':');
        let h: u32 = parts.next()?.parse().ok()?;
        let m: u32 = parts.next()?.parse().ok()?;
        Some(h * 60 + m)
    }

    let start_mins = parse_hhmm(&state.config.strategy.scan_start_time).unwrap_or(570);  // 09:30
    let end_mins   = parse_hhmm(&state.config.strategy.scan_end_time).unwrap_or(930);    // 15:30

    if et_mins < start_mins || et_mins >= end_mins {
        let _ = state.log_tx.send(LogEvent::info(format!(
            "outside scan window ({} – {}) ET — skipping",
            state.config.strategy.scan_start_time,
            state.config.strategy.scan_end_time,
        )));
        return false;
    }

    true
}

// ── Main strategy loop ────────────────────────────────────────────────────────

pub async fn run_strategy_loop(state: Arc<AppState>) {
    let entry_interval = Duration::from_secs(state.config.strategy.scan_interval_secs);

    let strategy = IronConduitStrategy::new();

    loop {
        {
            let status = state.status.lock().await;
            if *status == BotStatus::Stopping {
                drop(status);
                *state.status.lock().await = BotStatus::Idle;
                let _ = state.log_tx.send(LogEvent::info("strategy loop stopped"));
                return;
            }
        }

        // Market hours gate
        if !market_is_open(&state).await {
            sleep(entry_interval).await;
            continue;
        }

        let _ = state.log_tx.send(LogEvent::info("[iron-conduit] scan cycle starting"));

        match strategy.scan(&state).await {
            Ok(signals) if signals.is_empty() => {
                let _ = state.log_tx.send(LogEvent::info("[iron-conduit] no signals this cycle"));
            }
            Ok(signals) => {
                for signal in signals {
                    // Build real risk guard with live position count
                    let pos_count = state.open_positions.lock().await.len() as u32;
                    let guard = RiskGuard::new(&state.config, pos_count, 0.0, 1000.0, 0.0);
                    match guard.check_entry(&signal) {
                        Ok(()) => {
                            let _ = state.log_tx.send(LogEvent::risk("risk guard passed"));

                            // Check we're Running before submitting
                            let running = *state.status.lock().await == BotStatus::Running;
                            if !running {
                                let _ = state.log_tx.send(LogEvent::info("not running — skipping order"));
                                continue;
                            }

                            submit_signal_orders(&state, &signal).await;
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

/// Submit orders for all legs in a signal and track the position.
async fn submit_signal_orders(state: &AppState, signal: &Signal) {
    let (underlying, legs) = match signal {
        Signal::EnterLong  { symbol, legs } => (symbol.as_str(), legs),
        Signal::EnterShort { symbol, legs } => (symbol.as_str(), legs),
        Signal::Exit { symbol } => {
            let _ = state.log_tx.send(LogEvent::info(format!("exit signal for {symbol} — handled by exit monitor")));
            return;
        }
    };

    for leg in legs {
        let side = match leg.action {
            LegAction::Buy  => "buy",
            LegAction::Sell => "sell",
        };
        let limit_price = match leg.limit_price {
            Some(p) => p,
            None => {
                let _ = state.log_tx.send(LogEvent::warn(format!(
                    "leg {} has no limit price — skipping", leg.contract
                )));
                continue;
            }
        };

        match orders::submit_limit_order(&state.client, &leg.contract, side, leg.qty, limit_price).await {
            Ok(order) => {
                let _ = state.log_tx.send(LogEvent::order(format!(
                    "submitted {} {} {} @ ${:.2} → order id {}",
                    side, leg.qty, leg.contract, limit_price, order.id
                )));

                // Log to DB
                let direction = if leg.contract.contains('C') { "call" } else { "put" };
                let _ = state.db.insert_trade_log(
                    &leg.contract, underlying, direction, "buy",
                    limit_price, leg.qty as i64,
                    None, None, None, None, None, None, None,
                ).await;

                // Track position
                let meta = OpenPositionMeta {
                    contract:             leg.contract.clone(),
                    underlying:           underlying.to_string(),
                    direction:            direction.to_string(),
                    opened_at:            Utc::now(),
                    entry_price:          limit_price,
                    qty:                  leg.qty,
                    confluence_score:     0,
                    regime:               String::new(),
                    iv_rank:              0.0,
                    delta:                0.0,
                    dte_at_entry:         0,
                    pending_order_id:           Some(order.id),
                    pending_close_order_id:     None,
                    force_exit_next_open:       false,
                };
                state.open_positions.lock().await.insert(leg.contract.clone(), meta);
            }
            Err(e) => {
                let _ = state.log_tx.send(LogEvent::error(format!(
                    "order submit failed for {}: {e}", leg.contract
                )));
            }
        }
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

// ── Options API response types ────────────────────────────────────────────────

/// Response from GET /v2/options/contracts (Trading API)
#[derive(Debug, Deserialize)]
struct ContractsResponse {
    option_contracts: Vec<OptionContract>,
}

#[derive(Debug, Deserialize)]
struct OptionContract {
    symbol:          String,
    #[serde(rename = "type")]
    contract_type:   String,
    expiration_date: String,   // "YYYY-MM-DD"
    open_interest:   Option<f64>,
    tradable:        bool,
}

/// Response from GET /v1beta1/options/snapshots (Data API)
#[derive(Debug, Deserialize)]
struct OptionsSnapshotResponse {
    snapshots: std::collections::HashMap<String, OptionSnapshot>,
}

#[derive(Debug, Deserialize)]
struct OptionSnapshot {
    greeks:                  Option<Greeks>,
    #[serde(rename = "impliedVolatility")]
    iv:                      Option<f64>,
    #[serde(rename = "latestQuote")]
    quote:                   Option<OptionQuote>,
}

#[derive(Debug, Deserialize)]
struct Greeks {
    delta: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OptionQuote {
    #[serde(rename = "ap")] ask: Option<f64>,
    #[serde(rename = "bp")] bid: Option<f64>,
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
            .get_data_with_query(
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
                        let _ = state.log_tx.send(LogEvent::warn(format!("[iron-conduit] {symbol} bars fetch failed: {e}")));
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

            // ── Step 1: fetch contract list from Trading API ──────────────────
            let contracts_resp: ContractsResponse = match state.client
                .get_with_query(
                    "/v2/options/contracts",
                    &[
                        ("underlying_symbols", symbol),
                        ("expiration_date_gte", exp_min.as_str()),
                        ("expiration_date_lte", exp_max.as_str()),
                        ("type", contract_type_str),
                        ("status", "active"),
                        ("limit", "200"),
                    ],
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = state.log_tx.send(LogEvent::warn(format!("[iron-conduit] {symbol} contracts fetch failed: {e}")));
                    continue;
                }
            };

            // Pre-filter by tradable + DTE + open interest
            let today = Utc::now().date_naive();
            let filtered_contracts: Vec<&OptionContract> = contracts_resp.option_contracts.iter()
                .filter(|c| {
                    if !c.tradable { return false; }
                    let oi = c.open_interest.unwrap_or(0.0);
                    if oi < liq.min_open_interest as f64 { return false; }
                    if let Ok(exp) = chrono::NaiveDate::parse_from_str(&c.expiration_date, "%Y-%m-%d") {
                        let dte = (exp - today).num_days();
                        dte >= entry.dte_min as i64 && dte <= entry.dte_max as i64
                    } else {
                        false
                    }
                })
                .collect();

            if filtered_contracts.is_empty() {
                info!("[iron-conduit] {symbol}: no contracts passed DTE/OI filter");
                continue;
            }

            // ── Step 2: fetch snapshots from Data API (indicative = free feed) ─
            let symbols_csv: String = filtered_contracts.iter()
                .take(100)  // API limit per request
                .map(|c| c.symbol.as_str())
                .collect::<Vec<_>>()
                .join(",");

            let snapshot_resp: OptionsSnapshotResponse = match state.client
                .get_data_with_query(
                    "/v1beta1/options/snapshots",
                    &[("symbols", &symbols_csv), ("feed", "indicative")],
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = state.log_tx.send(LogEvent::warn(format!("[iron-conduit] {symbol} snapshots fetch failed: {e}")));
                    continue;
                }
            };

            // Build a lookup from contract symbol → open_interest for scoring
            let oi_map: std::collections::HashMap<&str, f64> = filtered_contracts.iter()
                .map(|c| (c.symbol.as_str(), c.open_interest.unwrap_or(0.0)))
                .collect();

            // Collect and rank qualifying contracts
            let mut candidates: Vec<(String, f64, f64, f64)> = Vec::new(); // (symbol, mid, delta_score, oi)

            for (contract, snap_opt) in &snapshot_resp.snapshots {
                let delta = snap_opt.greeks.as_ref().and_then(|g| g.delta).unwrap_or(0.0);
                let delta_abs = delta.abs();

                if delta_abs < entry.delta_min || delta_abs > entry.delta_max { continue; }

                let (bid, ask) = match &snap_opt.quote {
                    Some(q) => match (q.bid, q.ask) {
                        (Some(b), Some(a)) => (b, a),
                        _ => continue,
                    },
                    None => continue,
                };

                if ask - bid > liq.max_bid_ask_spread { continue; }
                if ask <= 0.0 { continue; }

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

                let delta_score = (delta_abs - entry.preferred_delta).abs();
                let oi = *oi_map.get(contract.as_str()).unwrap_or(&0.0);
                candidates.push((contract.clone(), mid, delta_score, oi));
            }

            // Rank: closest delta to preferred first, then highest OI
            candidates.sort_by(|a, b| {
                a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal)
                    .then(b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
            });

            if let Some((contract, mid, delta_score, oi)) = candidates.first() {
                let size_factor = cfg.sizing.size_factor_for(score);
                let iv_adj      = iv_engine.size_factor(0.0);
                let qty         = ((cfg.sizing.max_position_usd * size_factor * iv_adj)
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
                        contract:   contract.clone(),
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
