use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use serde::Deserialize;
use chrono::Utc;

use ferrum_core::{
    error::FerrumError,
    indicators,
    types::LogEvent,
};
use crate::{orders, AppState};

// ── Alpaca position response ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AlpacaPosition {
    pub symbol:          String,
    pub qty:             String,
    pub unrealized_pl:   String,
    pub unrealized_plpc: String,
    pub current_price:   String,
    pub cost_basis:      String,
}

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run_exit_monitor(state: Arc<AppState>) {
    let interval = Duration::from_secs(state.config.strategy.exit_check_interval);
    loop {
        sleep(interval).await;
        if let Err(e) = check_exits(&state).await {
            let _ = state.log_tx.send(LogEvent::error(format!("exit monitor: {e}")));
        }
    }
}

// ── Core exit logic ───────────────────────────────────────────────────────────

async fn check_exits(state: &AppState) -> Result<(), FerrumError> {
    let alpaca_positions: Vec<AlpacaPosition> = match state.client.get("/v2/positions").await {
        Ok(v)  => v,
        Err(e) => {
            let _ = state.log_tx.send(LogEvent::warn(format!("exit monitor: /v2/positions failed: {e}")));
            return Ok(());
        }
    };

    // Cache of underlying → EMA50 value, fetched at most once per cycle
    let mut ema50_cache: HashMap<String, Option<f64>> = HashMap::new();

    for ap in &alpaca_positions {
        let qty: f64            = ap.qty.parse().unwrap_or(0.0);
        let unrealized_pl: f64  = ap.unrealized_pl.parse().unwrap_or(0.0);
        let unrealized_plpc: f64 = ap.unrealized_plpc.parse().unwrap_or(0.0);
        let current_price: f64  = ap.current_price.parse().unwrap_or(0.0);
        let cost_basis: f64     = ap.cost_basis.parse().unwrap_or(0.0);
        let pnl_pct = unrealized_plpc * 100.0;

        let meta = {
            let positions = state.open_positions.lock().await;
            positions.get(&ap.symbol).cloned()
        };
        let meta = match meta {
            Some(m) => m,
            None    => continue,
        };

        // Skip if we already have a pending close order out
        if meta.pending_close_order_id.is_some() {
            continue;
        }

        let exit_cfg  = &state.config.strategy.exit;
        let days_held = (Utc::now() - meta.opened_at).num_days();
        let dte       = dte_from_occ_symbol(&ap.symbol).unwrap_or(99);

        // ── EMA50 break check (cached per underlying per cycle) ───────────────
        let ema50_broken = {
            let ema50 = fetch_ema50_cached(state, &meta.underlying, &mut ema50_cache).await;
            match (ema50, meta.direction.as_str()) {
                (Some(e), "call") => current_price < e,  // underlying broke below EMA50 → call thesis dead
                (Some(e), "put")  => current_price > e,  // underlying broke above EMA50 → put thesis dead
                _                 => false,
            }
        };

        // ── Update peak P&L for trailing profit target ────────────────────────
        {
            let mut positions = state.open_positions.lock().await;
            if let Some(m) = positions.get_mut(&ap.symbol) {
                if pnl_pct > m.peak_pnl_pct {
                    m.peak_pnl_pct = pnl_pct;
                }
            }
        }
        let peak_pnl = meta.peak_pnl_pct.max(pnl_pct);  // use updated peak

        // ── Trailing profit target ────────────────────────────────────────────
        // Activates once P&L >= trailing_activation_pct.
        // Closes when P&L drops trailing_trail_gap_pct below observed peak.
        let trailing_triggered = peak_pnl >= exit_cfg.trailing_activation_pct
            && pnl_pct <= peak_pnl - exit_cfg.trailing_trail_gap_pct;

        // ── Exit priority order ───────────────────────────────────────────────
        let exit_reason: Option<&str> = if pnl_pct <= -(exit_cfg.stop_loss_pct) {
            Some("stop_loss")
        } else if dte <= exit_cfg.theta_exit_dte && pnl_pct < exit_cfg.theta_exit_min_pnl_pct {
            Some("theta_exit")
        } else if trailing_triggered {
            Some("trailing_profit")
        } else if ema50_broken {
            Some("ema50_break")
        } else if dte <= exit_cfg.time_exit_dte {
            Some("time_exit")
        } else if days_held >= exit_cfg.dead_money_days as i64
               && pnl_pct < exit_cfg.dead_money_min_pct
        {
            Some("dead_money")
        } else {
            None
        };

        let exit_reason = match exit_reason {
            Some(r) => r,
            None    => continue,
        };

        let _ = state.log_tx.send(LogEvent::info(format!(
            "exit monitor: {} → {} (pnl={:.1}% peak={:.1}% dte={} days_held={})",
            ap.symbol, exit_reason, pnl_pct, peak_pnl, dte, days_held,
        )));

        // ── PDT gate ──────────────────────────────────────────────────────────
        let pdt_check = {
            let pdt = state.pdt.lock().await;
            pdt.check_exit_allowed(meta.opened_at, pnl_pct)
        };

        if let Err(msg) = pdt_check {
            let _ = state.log_tx.send(LogEvent::warn(format!(
                "{} — holding overnight ({})", ap.symbol, msg
            )));
            let mut positions = state.open_positions.lock().await;
            if let Some(m) = positions.get_mut(&ap.symbol) {
                m.force_exit_next_open = true;
            }
            continue;
        }

        // ── Submit close order ────────────────────────────────────────────────
        let close_qty  = qty.abs() as u32;
        let close_side = if qty > 0.0 { "sell" } else { "buy" };

        match orders::submit_limit_order(&state.client, &ap.symbol, close_side, close_qty, current_price).await {
            Ok(order) => {
                let _ = state.log_tx.send(LogEvent::order(format!(
                    "CLOSE submitted: {} x{} @ ${:.2} reason={exit_reason} order={}",
                    ap.symbol, close_qty, current_price, order.id,
                )));

                // Write DB open-close record (preliminary — will be confirmed by order poller)
                let entry_price = if cost_basis > 0.0 && qty > 0.0 {
                    cost_basis / (qty * 100.0)
                } else {
                    meta.entry_price
                };
                let est_pnl = (current_price - entry_price) * close_qty as f64 * 100.0;
                let _ = state.db.insert_trade_log(
                    &ap.symbol, &meta.underlying, &meta.direction,
                    "close_pending", current_price, close_qty as i64,
                    Some(meta.confluence_score as i64),
                    Some(meta.regime.as_str()),
                    Some(meta.iv_rank),
                    Some(meta.delta),
                    Some(dte as i64),
                    Some(exit_reason),
                    Some(est_pnl),
                ).await;

                // Set pending_close_order_id — order poller confirms the fill
                let mut positions = state.open_positions.lock().await;
                if let Some(m) = positions.get_mut(&ap.symbol) {
                    m.pending_close_order_id = Some(order.id);
                }
            }
            Err(e) => {
                let _ = state.log_tx.send(LogEvent::error(format!(
                    "exit monitor: close order failed for {}: {e}", ap.symbol
                )));
            }
        }
    }

    Ok(())
}

// ── EMA50 helper ──────────────────────────────────────────────────────────────

async fn fetch_ema50_cached(
    state: &AppState,
    underlying: &str,
    cache: &mut HashMap<String, Option<f64>>,
) -> Option<f64> {
    if let Some(cached) = cache.get(underlying) {
        return *cached;
    }

    let result = fetch_ema50(state, underlying).await;
    cache.insert(underlying.to_string(), result);
    result
}

async fn fetch_ema50(state: &AppState, underlying: &str) -> Option<f64> {
    let start = (Utc::now() - chrono::Duration::days(90))
        .format("%Y-%m-%dT00:00:00Z").to_string();

    #[derive(Deserialize)]
    struct BarsResp { bars: Vec<Bar> }
    #[derive(Deserialize)]
    struct Bar { #[serde(rename = "c")] close: f64 }

    let resp: BarsResp = state.client
        .get_data_with_query(
            &format!("/v2/stocks/{underlying}/bars"),
            &[("timeframe", "1Day"), ("start", &start), ("limit", "100"), ("feed", "iex")],
        )
        .await
        .ok()?;

    if resp.bars.len() < 50 {
        return None;
    }

    let closes: Vec<f64> = resp.bars.iter().map(|b| b.close).collect();
    let ema50 = indicators::ema_last(&closes, 50);
    if ema50.is_nan() { None } else { Some(ema50) }
}

// ── OCC symbol DTE parser ─────────────────────────────────────────────────────

/// Parse DTE from OCC option symbol: <underlying><YYMMDD><C/P><strike*1000>
pub fn dte_from_occ_symbol(symbol: &str) -> Option<u32> {
    let cp_pos = symbol.bytes().position(|b| b == b'C' || b == b'P')?;
    if cp_pos < 6 { return None; }
    let date_str = &symbol[cp_pos - 6..cp_pos];
    let yy: i32 = date_str[0..2].parse().ok()?;
    let mm: u32 = date_str[2..4].parse().ok()?;
    let dd: u32 = date_str[4..6].parse().ok()?;
    let expiry = chrono::NaiveDate::from_ymd_opt(2000 + yy, mm, dd)?;
    let days = (expiry - Utc::now().date_naive()).num_days();
    Some(days.max(0) as u32)
}
