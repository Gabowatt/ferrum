use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};
use serde::Deserialize;
use chrono::Utc;

use ferrum_core::{error::FerrumError, types::LogEvent};
use crate::{orders, pdt::DayTradeRecord, AppState};

// ── Alpaca position response ─────────────────────────────────────────────────

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

// ── Core logic ────────────────────────────────────────────────────────────────

async fn check_exits(state: &AppState) -> Result<(), FerrumError> {
    // Fetch live Alpaca positions
    let alpaca_positions: Vec<AlpacaPosition> = match state.client.get("/v2/positions").await {
        Ok(v)  => v,
        Err(e) => {
            warn!("exit monitor: could not fetch /v2/positions: {e}");
            return Ok(());
        }
    };

    for ap in &alpaca_positions {
        let qty: f64 = ap.qty.parse().unwrap_or(0.0);
        let unrealized_pl: f64    = ap.unrealized_pl.parse().unwrap_or(0.0);
        let unrealized_plpc: f64  = ap.unrealized_plpc.parse().unwrap_or(0.0);
        let current_price: f64    = ap.current_price.parse().unwrap_or(0.0);
        let cost_basis: f64       = ap.cost_basis.parse().unwrap_or(0.0);
        let pnl_pct = unrealized_plpc * 100.0; // e.g. 13.3 for +13.3%

        // Match against our tracked positions
        let meta = {
            let positions = state.open_positions.lock().await;
            positions.get(&ap.symbol).cloned()
        };

        let meta = match meta {
            Some(m) => m,
            None    => continue, // position not opened by us this session — skip
        };

        let exit_cfg   = &state.config.strategy.exit;
        let days_held  = (Utc::now() - meta.opened_at).num_days();

        // ── Compute DTE ───────────────────────────────────────────────────────
        // Parse expiration from OCC symbol: last 6 digits before C/P are YYMMDD
        let dte = dte_from_occ_symbol(&ap.symbol).unwrap_or(99);

        // ── Exit condition checks (priority order) ────────────────────────────

        let exit_reason: Option<&str> = {
            if pnl_pct <= -(exit_cfg.stop_loss_pct) {
                Some("stop_loss")
            } else if dte <= 7 && pnl_pct < 10.0 {
                Some("dte_7_low_pnl")
            } else if pnl_pct >= exit_cfg.profit_target_single_pct {
                Some("profit_target_single")
            } else if dte as u32 <= exit_cfg.time_exit_dte {
                Some("dte_time_exit")
            } else if days_held >= exit_cfg.dead_money_days as i64
                && pnl_pct < exit_cfg.dead_money_min_pct
            {
                Some("dead_money")
            } else {
                None
            }
        };

        let exit_reason = match exit_reason {
            Some(r) => r,
            None    => continue,
        };

        info!("exit monitor: {} → exit reason: {} (pnl={:.1}%, dte={}, days_held={})",
            ap.symbol, exit_reason, pnl_pct, dte, days_held);

        // ── PDT check ─────────────────────────────────────────────────────────
        let pdt_ok = {
            let pdt = state.pdt.lock().await;
            pdt.check_exit_allowed(meta.opened_at, pnl_pct)
        };

        if let Err(pdt_msg) = pdt_ok {
            let _ = state.log_tx.send(LogEvent::warn(format!(
                "holding overnight — PDT blocked exit for {}: {pdt_msg}", ap.symbol
            )));
            // Set force_exit_next_open flag
            let mut positions = state.open_positions.lock().await;
            if let Some(m) = positions.get_mut(&ap.symbol) {
                m.force_exit_next_open = true;
            }
            continue;
        }

        // ── Submit close order ────────────────────────────────────────────────
        let close_qty = qty.abs() as u32;
        let close_side = if qty > 0.0 { "sell" } else { "buy" };

        match orders::submit_limit_order(
            &state.client, &ap.symbol, close_side, close_qty, current_price,
        ).await {
            Ok(order) => {
                let _ = state.log_tx.send(LogEvent::order(format!(
                    "close order submitted: {} {} @ ${:.2} (reason: {exit_reason}) → {}",
                    ap.symbol, close_qty, current_price, order.id
                )));

                // Write DB close record
                let _ = state.db.insert_trade_log(
                    &ap.symbol,
                    &meta.underlying,
                    &meta.direction,
                    "sell",
                    current_price,
                    close_qty as i64,
                    None,
                    None,
                    None,
                    None,
                    Some(dte as i64),
                    Some(exit_reason),
                    Some(unrealized_pl),
                ).await;

                // Record day trade if applicable
                let is_day_trade = {
                    let pdt = state.pdt.lock().await;
                    pdt.would_be_day_trade(meta.opened_at)
                };
                if is_day_trade {
                    let entry_price = if cost_basis > 0.0 && qty > 0.0 {
                        cost_basis / (qty * 100.0) // cost_basis is total cost for options
                    } else {
                        meta.entry_price
                    };
                    let dt_record = DayTradeRecord {
                        contract_symbol: ap.symbol.clone(),
                        underlying:      meta.underlying.clone(),
                        open_time:       meta.opened_at,
                        close_time:      Utc::now(),
                        open_price:      entry_price,
                        close_price:     current_price,
                        pnl:             unrealized_pl,
                        was_emergency:   exit_reason == "stop_loss",
                    };
                    let _ = state.db.insert_day_trade(&dt_record).await;
                    let mut pdt = state.pdt.lock().await;
                    pdt.record(dt_record);
                }

                // Remove from tracked positions
                state.open_positions.lock().await.remove(&ap.symbol);
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

/// Parse DTE from an OCC-style option symbol.
/// Format: <underlying><YYMMDD><C/P><strike*1000>
/// e.g. AAPL260117C00200000 → expiry 2026-01-17
fn dte_from_occ_symbol(symbol: &str) -> Option<u32> {
    // Find the C or P marker (first occurrence)
    let cp_pos = symbol.bytes().position(|b| b == b'C' || b == b'P')?;
    if cp_pos < 6 {
        return None;
    }
    let date_str = &symbol[cp_pos - 6..cp_pos]; // YYMMDD
    if date_str.len() != 6 {
        return None;
    }
    let yy: i32 = date_str[0..2].parse().ok()?;
    let mm: u32 = date_str[2..4].parse().ok()?;
    let dd: u32 = date_str[4..6].parse().ok()?;
    let year = 2000 + yy;
    let expiry = chrono::NaiveDate::from_ymd_opt(year, mm, dd)?;
    let today  = Utc::now().date_naive();
    let days = (expiry - today).num_days();
    Some(days.max(0) as u32)
}
