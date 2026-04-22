use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use tracing::error;
use serde::Deserialize;
use chrono::{Datelike, Timelike, Utc};

use ferrum_core::{
    client::AlpacaClient,
    config::AppConfig,
    error::FerrumError,
    indicators::{self, BarContext, TradeDirection},
    types::{BotStatus, FillRecord, LegAction, LogEvent, OptionLeg, OrderType, Signal},
};
use crate::{
    iv_rank::IvRankEngine,
    orders,
    risk::RiskGuard,
    AppState, OpenPositionMeta,
};

// ── Strategy trait + registry ─────────────────────────────────────────────────
//
// V2.1 multi-strategy refactor: the daemon hosts a registry of strategies
// (`AppState.strategies`), each running on its own loop. Phase 1 ships with
// one strategy (Forge); Phase 3 adds Iron Condor.
//
// `Strategy` is the trait every strategy implements. `StrategyHandle` wraps
// an `Arc<dyn Strategy>` together with runtime state the supervisor needs
// (scan interval, enabled flag for Phase 2 live toggles).

#[async_trait::async_trait]
pub trait Strategy: Send + Sync {
    /// Stable, lowercase identifier — used as `strategy_id` in DB rows and
    /// in IPC payloads. Must match the `[strategies.<id>]` section that will
    /// be introduced in Phase 2 of the multi-strategy plan.
    fn id(&self) -> &'static str;

    async fn scan(&self, state: &AppState) -> Result<Vec<Signal>, FerrumError>;
}

pub struct StrategyHandle {
    pub id:            &'static str,
    pub scan_interval: Duration,
    /// Live enable/disable. Phase 1 always starts enabled; Phase 2 wires the
    /// IPC + UI toggle that flips this without restarting the daemon.
    pub enabled:       AtomicBool,
    pub strategy:      Arc<dyn Strategy>,
}

impl StrategyHandle {
    pub fn new(strategy: Arc<dyn Strategy>, scan_interval: Duration) -> Arc<Self> {
        Arc::new(Self {
            id: strategy.id(),
            scan_interval,
            enabled: AtomicBool::new(true),
            strategy,
        })
    }

    pub fn is_enabled(&self) -> bool { self.enabled.load(Ordering::Relaxed) }

    #[allow(dead_code)] // wired in Phase 2 when SetStrategyEnabled IPC lands
    pub fn set_enabled(&self, v: bool) { self.enabled.store(v, Ordering::Relaxed); }
}

impl std::fmt::Debug for StrategyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrategyHandle")
            .field("id", &self.id)
            .field("scan_interval", &self.scan_interval)
            .field("enabled", &self.is_enabled())
            .finish_non_exhaustive()
    }
}

/// Build the list of strategy handles registered with the daemon.
///
/// Phase 1: only Forge. Phase 3 will add Iron Condor here. The function reads
/// each strategy's scan interval from `config` so the loop schedule stays
/// configurable without touching code.
pub fn build_strategies(config: &AppConfig) -> Vec<Arc<StrategyHandle>> {
    let forge_interval = Duration::from_secs(config.strategy.scan_interval_secs);
    vec![
        StrategyHandle::new(Arc::new(ForgeStrategy::new()), forge_interval),
    ]
}

// ── Alpaca clock ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AlpacaClock {
    is_open:    bool,
    next_open:  Option<String>,
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

// ── Per-strategy supervisor loop ──────────────────────────────────────────────
//
// One task per `StrategyHandle`, spawned by `IpcCommand::Start`. The loop
// scans on the handle's interval, gates on `BotStatus::Running` and the
// per-handle `enabled` flag, and exits when status becomes `Stopping`.
//
// Stop coordination across N loops:
//   - `state.active_strategy_loops` counts the number of currently-running
//     supervisor tasks. Each loop increments on entry and decrements on exit.
//   - The IPC Stop handler sets status to `Stopping` and pings `stop_notify`,
//     which wakes every loop's interruptible sleep.
//   - The last loop to exit (post-decrement counter == 0) flips status from
//     `Stopping` back to `Idle`. This avoids a race where each loop tried to
//     write `Idle` independently.

pub async fn run_strategy_loop(handle: Arc<StrategyHandle>, state: Arc<AppState>) {
    state.active_strategy_loops.fetch_add(1, Ordering::SeqCst);
    let strategy_id = handle.id;

    loop {
        // Stop check — break out before any work this cycle.
        {
            let status = state.status.lock().await;
            if *status == BotStatus::Stopping {
                break;
            }
        }

        // Per-handle enable flag (Phase 2 will toggle this live).
        if !handle.is_enabled() {
            interruptible_sleep(&state, handle.scan_interval).await;
            continue;
        }

        // Market hours gate
        if !market_is_open(&state).await {
            interruptible_sleep(&state, handle.scan_interval).await;
            continue;
        }

        let _ = state.log_tx.send(LogEvent::info(format!("[{strategy_id}] scan cycle starting")));

        match handle.strategy.scan(&state).await {
            Ok(signals) if signals.is_empty() => {
                let _ = state.log_tx.send(LogEvent::info(format!("[{strategy_id}] no signals this cycle")));
            }
            Ok(signals) => {
                for signal in signals {
                    // Build real risk guard with live position count + sector map.
                    let (pos_count, open_underlyings) = {
                        let positions = state.open_positions.lock().await;
                        let underlyings: Vec<String> = positions.values()
                            .map(|m| m.underlying.clone())
                            .collect();
                        (positions.len() as u32, underlyings)
                    };
                    let guard = RiskGuard::new(&state.config, pos_count, 0.0, 1000.0, 0.0)
                        .with_open_underlyings(&open_underlyings);
                    match guard.check_entry(&signal) {
                        Ok(()) => {
                            let _ = state.log_tx.send(LogEvent::risk(format!("[{strategy_id}] risk guard passed")));

                            // Check we're Running before submitting
                            let running = *state.status.lock().await == BotStatus::Running;
                            if !running {
                                let _ = state.log_tx.send(LogEvent::info(format!("[{strategy_id}] not running — skipping order")));
                                continue;
                            }

                            submit_signal_orders(&state, &signal).await;
                        }
                        Err(e) => {
                            let _ = state.log_tx.send(LogEvent::risk(format!("[{strategy_id}] blocked: {e}")));
                        }
                    }
                }
            }
            Err(e) => {
                error!("[{strategy_id}] scan error: {e}");
                let _ = state.log_tx.send(LogEvent::error(format!("[{strategy_id}] scan error: {e}")));
            }
        }

        interruptible_sleep(&state, handle.scan_interval).await;
    }

    // Last loop out flips status to Idle.
    let remaining = state.active_strategy_loops.fetch_sub(1, Ordering::SeqCst) - 1;
    let _ = state.log_tx.send(LogEvent::info(format!(
        "[{strategy_id}] supervisor exited ({remaining} loop(s) still running)"
    )));
    if remaining == 0 {
        *state.status.lock().await = BotStatus::Idle;
        let _ = state.log_tx.send(LogEvent::info("all strategy loops stopped"));
    }
}

/// Sleep until either `dur` elapses or a Stop is requested via `state.stop_notify`.
/// Lets the Stop button take effect immediately instead of waiting out the scan interval.
async fn interruptible_sleep(state: &AppState, dur: Duration) {
    tokio::select! {
        _ = sleep(dur) => {}
        _ = state.stop_notify.notified() => {}
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
                    peak_pnl_pct:              f64::NEG_INFINITY,
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
    // Alpaca Algo Trader Plus: 10k req/min. Fast fill sync keeps trade_log fresh.
    let mut interval = tokio::time::interval(Duration::from_secs(15));
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
    expiration_date: String,           // "YYYY-MM-DD"
    open_interest:   Option<String>,   // API returns string e.g. "17"
    tradable:        bool,
}

impl OptionContract {
    /// Parsed open_interest, or None if Alpaca didn't return a value (early in
    /// the session this can be missing for otherwise-liquid strikes).
    fn open_interest_opt(&self) -> Option<f64> {
        self.open_interest.as_deref().and_then(|s| s.parse().ok())
    }
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

// ── Forge Strategy ────────────────────────────────────────────────────────────
// Long single-leg calls/puts on confluence + regime. Renamed from "iron conduit"
// in V2.1 — the old name was misleading because the code does not trade iron
// condors. The true 4-leg condor is being added as a separate strategy.

pub struct ForgeStrategy;

impl ForgeStrategy {
    pub fn new() -> Self { Self }

    async fn fetch_bars(
        client: &AlpacaClient,
        symbol: &str,
        days: u32,
    ) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>), FerrumError> {
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
        let opens:   Vec<f64> = resp.bars.iter().map(|b| b.open).collect();
        Ok((closes, highs, lows, volumes, opens))
    }

    /// Build a BarContext for confluence scoring from the raw bars arrays.
    fn make_bar_context(
        closes:  &[f64],
        highs:   &[f64],
        lows:    &[f64],
        opens:   &[f64],
        volumes: &[f64],
    ) -> Option<BarContext> {
        let n = closes.len();
        if n < 21 { return None; }

        let high  = *highs.last()?;
        let low   = *lows.last()?;
        let open  = *opens.last()?;

        // 5 bars ago (index n-6)
        let low_5b_ago  = lows.get(n.wrapping_sub(6)).copied().unwrap_or(low);
        let high_5b_ago = highs.get(n.wrapping_sub(6)).copied().unwrap_or(high);

        // 20-day rolling extremes (excluding today)
        let window_start = n.saturating_sub(21);
        let high_20d = highs[window_start..n-1].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let low_20d  = lows[window_start..n-1].iter().cloned().fold(f64::INFINITY, f64::min);

        // MACD histogram from the previous bar
        let macd_hist_prev = if n >= 2 {
            let m = indicators::macd_last(&closes[..n-1], 12, 26, 9);
            m.histogram
        } else {
            f64::NAN
        };

        // Volume ratio is not directly needed here (it's in the snapshot), but included
        // so that future callers can use it without recomputing.
        let _ = volumes; // suppress unused warning

        Some(BarContext {
            high, low, open,
            low_5b_ago, high_5b_ago,
            high_20d, low_20d,
            macd_hist_prev,
        })
    }
}

#[async_trait::async_trait]
impl Strategy for ForgeStrategy {
    fn id(&self) -> &'static str { "forge" }

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

        let cooldown = Duration::from_millis(
            (cfg.strategy.market_data_cooldown * 1000) as u64
        );

        for symbol in all_symbols {
            sleep(cooldown).await;

            // ── Cooldown veto: skip if we closed this underlying recently ────────
            let cooldown_hours = entry.cooldown_after_close_hours;
            {
                let last_close_map = state.last_close_by_underlying.lock().await;
                if let Some(&closed_at) = last_close_map.get(symbol) {
                    let hours_since = (Utc::now() - closed_at).num_minutes() as f64 / 60.0;
                    if hours_since < cooldown_hours {
                        let _ = state.log_tx.send(LogEvent::info(format!(
                            "[forge] {symbol}: cooldown ({:.1}h < {:.1}h) — skip",
                            hours_since, cooldown_hours,
                        )));
                        continue;
                    }
                }
            }

            // Fetch daily bars for indicator computation
            let (closes, highs, lows, volumes, opens) =
                match Self::fetch_bars(&state.client, symbol, 90).await {
                    Ok(data) => data,
                    Err(e) => {
                        let _ = state.log_tx.send(LogEvent::warn(format!("[forge] {symbol} bars fetch failed: {e}")));
                        continue;
                    }
                };

            if closes.len() < 60 {
                let _ = state.log_tx.send(LogEvent::info(format!(
                    "[forge] {symbol}: insufficient bar history ({} bars)", closes.len(),
                )));
                continue;
            }

            // Compute indicators
            let snap = match indicators::compute_snapshot(
                &closes, &highs, &lows, &volumes,
                rc.adx_trend_threshold, rc.adx_no_trend_threshold,
                entry.bb_width_min_pct, entry.ema_slope_lookback_bars,
            ) {
                Some(s) => s,
                None => {
                    let _ = state.log_tx.send(LogEvent::warn(format!(
                        "[forge] {symbol}: snapshot failed",
                    )));
                    continue;
                }
            };

            // Build bar context for v2.1 scoring
            let ctx = match Self::make_bar_context(&closes, &highs, &lows, &opens, &volumes) {
                Some(c) => c,
                None => {
                    let _ = state.log_tx.send(LogEvent::warn(format!("[forge] {symbol}: bar context failed")));
                    continue;
                }
            };

            let _ = state.log_tx.send(LogEvent::info(format!(
                "[forge] {symbol}: regime={} ema9={:.2} ema20={:.2} rsi={:.1} adx={:.1} bb_width={:.1}%",
                snap.regime, snap.ema9, snap.ema20, snap.rsi, snap.adx.adx,
                snap.bbands.width * 100.0,
            )));

            // ── Confluence gate (v2.1 regime-specific scoring) ───────────────
            let (score, max_score, direction) = match indicators::confluence_score(
                &snap, &ctx, entry.allow_choppy,
            ) {
                Some(s) => s,
                None => {
                    // Choppy regime and allow_choppy = false
                    let _ = state.log_tx.send(LogEvent::info(format!(
                        "[forge] {symbol}: choppy regime — no trade (allow_choppy=false)",
                    )));
                    let _ = state.db.insert_scan_result(
                        symbol, &snap.regime.to_string(), 0, None, "choppy",
                    ).await;
                    continue;
                }
            };

            let dir_str = match direction {
                TradeDirection::Call => "call",
                TradeDirection::Put  => "put",
            };

            // Regime-specific minimum score gate
            use ferrum_core::indicators::Regime;
            let min_score = match snap.regime {
                Regime::TrendingUp | Regime::TrendingDown => entry.trend_min_score,
                Regime::RangeBound                        => entry.range_min_score,
                Regime::Choppy                            => entry.choppy_min_score,
            };

            let _ = state.log_tx.send(LogEvent::info(format!(
                "[forge] {symbol}: score={score}/{max_score} min={min_score} dir={dir_str} regime={}",
                snap.regime,
            )));

            if score < min_score {
                let _ = state.log_tx.send(LogEvent::info(format!(
                    "[forge] {symbol}: score {score} < min {min_score} — skip",
                )));
                let _ = state.db.insert_scan_result(
                    symbol, &snap.regime.to_string(), score as i32,
                    Some(dir_str), "below_threshold",
                ).await;
                continue;
            }

            // ── Extreme proximity veto ───────────────────────────────────────
            // Reject if today's high (for calls) is within `extreme_proximity_atr` ATRs
            // of the 20-day high — buying at the local extreme is a high-probability stop-out.
            if !snap.atr.is_nan() && snap.atr > 0.0 {
                let proximity_threshold = entry.extreme_proximity_atr * snap.atr;
                let vetoed = match direction {
                    TradeDirection::Call => ctx.high_20d - ctx.high < proximity_threshold,
                    TradeDirection::Put  => ctx.low - ctx.low_20d  < proximity_threshold,
                };
                if vetoed {
                    let _ = state.log_tx.send(LogEvent::info(format!(
                        "[forge] {symbol}: extreme proximity veto ({dir_str} near 20d extreme) — skip",
                    )));
                    let _ = state.db.insert_scan_result(
                        symbol, &snap.regime.to_string(), score as i32,
                        Some(dir_str), "extreme_proximity",
                    ).await;
                    continue;
                }
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
                    let _ = state.log_tx.send(LogEvent::warn(format!("[forge] {symbol} contracts fetch failed: {e}")));
                    continue;
                }
            };

            // Pre-filter by tradable + DTE + open interest.
            // OI missing from Alpaca (None) is treated as "unknown — allow" so
            // otherwise-liquid strikes aren't silently dropped early in the session;
            // the bid/ask spread check in Step 2 is the real liquidity gate.
            let today = Utc::now().date_naive();
            let total_returned = contracts_resp.option_contracts.len();
            let filtered_contracts: Vec<&OptionContract> = contracts_resp.option_contracts.iter()
                .filter(|c| {
                    if !c.tradable { return false; }
                    if let Some(oi) = c.open_interest_opt() {
                        if oi < liq.min_open_interest as f64 { return false; }
                    }
                    if let Ok(exp) = chrono::NaiveDate::parse_from_str(&c.expiration_date, "%Y-%m-%d") {
                        let dte = (exp - today).num_days();
                        dte >= entry.dte_min as i64 && dte <= entry.dte_max as i64
                    } else {
                        false
                    }
                })
                .collect();

            if filtered_contracts.is_empty() {
                let _ = state.log_tx.send(LogEvent::info(format!(
                    "[forge] {symbol}: no contracts passed DTE/OI filter \
                     ({total_returned} returned by Alpaca, DTE {}-{}, OI≥{})",
                    entry.dte_min, entry.dte_max, liq.min_open_interest,
                )));
                let _ = state.db.insert_scan_result(
                    symbol, &snap.regime.to_string(), score as i32,
                    Some(dir_str), "no_contracts",
                ).await;
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
                    let _ = state.log_tx.send(LogEvent::warn(format!("[forge] {symbol} snapshots fetch failed: {e}")));
                    continue;
                }
            };

            // Build a lookup from contract symbol → open_interest for scoring.
            // Unknown OI (Alpaca returned null) → 0.0 here; scoring just uses
            // it as a tie-breaker so a 0 is fine.
            let oi_map: std::collections::HashMap<&str, f64> = filtered_contracts.iter()
                .map(|c| (c.symbol.as_str(), c.open_interest_opt().unwrap_or(0.0)))
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
                    .unwrap_or_else(|_| crate::iv_rank::IvRankResult { iv_rank: 50.0 });

                if cfg.symbols.tier_of(symbol) == Some(3)
                    && iv_result.iv_rank < cfg.symbols.tier3_iv_rank_min
                {
                    continue;
                }

                if !iv_engine.is_buyable(iv_result.iv_rank) {
                    let _ = state.log_tx.send(LogEvent::info(format!(
                        "[forge] {symbol} {contract}: IV rank {:.1} too high — skip",
                        iv_result.iv_rank,
                    )));
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
                // v2.1 regime-specific size factors
                let size_factor = {
                    use ferrum_core::indicators::Regime;
                    match snap.regime {
                        Regime::Choppy => 0.5,  // always half size in choppy
                        Regime::RangeBound => match score {
                            s if s >= 9  => 1.0,
                            s if s >= 7  => 0.75,
                            _            => 0.5,
                        },
                        Regime::TrendingUp | Regime::TrendingDown => match score {
                            s if s >= 11 => 1.0,
                            s if s >= 9  => 0.75,
                            _            => 0.5,
                        },
                    }
                };
                let iv_adj      = iv_engine.size_factor(0.0);
                let qty         = ((cfg.sizing.max_position_usd * size_factor * iv_adj)
                    / (mid * 100.0))
                    .floor() as u32;
                let qty = qty.max(1);

                let action = match direction {
                    TradeDirection::Call | TradeDirection::Put => LegAction::Buy,
                };

                let _ = state.log_tx.send(LogEvent::info(format!(
                    "[forge] SIGNAL {symbol} {contract} dir={dir_str} \
                     mid=${mid:.2} score={score}/{max_score} size={size_factor:.2} delta_dist={delta_score:.3} oi={oi:.0} qty={qty}",
                )));
                let _ = state.db.insert_scan_result(
                    symbol, &snap.regime.to_string(), score as i32,
                    Some(dir_str), "entered",
                ).await;

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
            } else {
                let _ = state.db.insert_scan_result(
                    symbol, &snap.regime.to_string(), score as i32,
                    Some(dir_str), "no_contracts",
                ).await;
            }
        }

        Ok(signals)
    }
}
