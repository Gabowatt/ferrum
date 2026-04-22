use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use chrono::Utc;

use ferrum_core::types::LogEvent;
use crate::{orders, pdt::DayTradeRecord, AppState};

const POLL_INTERVAL_SECS: u64 = 30;

pub async fn run_order_poller(state: Arc<AppState>) {
    let interval = Duration::from_secs(POLL_INTERVAL_SECS);
    loop {
        sleep(interval).await;
        if let Err(e) = poll_orders(&state).await {
            let _ = state.log_tx.send(LogEvent::error(format!("order poller: {e}")));
        }
    }
}

async fn poll_orders(state: &AppState) -> Result<(), ferrum_core::error::FerrumError> {
    // Fetch all currently open orders from Alpaca
    let open_orders = orders::get_open_orders(&state.client).await?;
    let open_order_ids: std::collections::HashSet<&str> =
        open_orders.iter().map(|o| o.id.as_str()).collect();

    // Snapshot the contracts we're tracking to avoid holding the lock during API calls
    let tracked: Vec<(String, crate::OpenPositionMeta)> = {
        state.open_positions.lock().await
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };

    for (contract, meta) in tracked {
        // ── Entry order check ──────────────────────────────────────────────────
        if let Some(ref entry_id) = meta.pending_order_id {
            if !open_order_ids.contains(entry_id.as_str()) {
                // No longer open — fetch final status
                match orders::get_order(&state.client, entry_id).await {
                    Ok(order) => match order.status.as_str() {
                        "filled" | "partially_filled" => {
                            let filled_price = order.filled_avg_price
                                .as_deref()
                                .and_then(|p| p.parse::<f64>().ok())
                                .unwrap_or(meta.entry_price);

                            let _ = state.log_tx.send(LogEvent::order(format!(
                                "FILLED {} x{} @ ${:.2} (entry)",
                                contract,
                                order.filled_qty.parse::<u32>().unwrap_or(meta.qty),
                                filled_price,
                            )));

                            let mut positions = state.open_positions.lock().await;
                            if let Some(m) = positions.get_mut(&contract) {
                                m.entry_price      = filled_price;
                                m.pending_order_id = None;
                            }
                        }
                        "canceled" | "expired" | "replaced" => {
                            let _ = state.log_tx.send(LogEvent::warn(format!(
                                "entry order {} for {contract} was {} — removing position",
                                entry_id, order.status
                            )));
                            state.open_positions.lock().await.remove(&contract);
                        }
                        other => {
                            let _ = state.log_tx.send(LogEvent::warn(format!("order {entry_id} for {contract} has unexpected status: {other}")));
                        }
                    },
                    Err(e) => {
                        let _ = state.log_tx.send(LogEvent::warn(format!("could not fetch entry order {entry_id}: {e}")));
                    }
                }
            }
        }

        // ── Close order check ──────────────────────────────────────────────────
        if let Some(ref close_id) = meta.pending_close_order_id {
            if !open_order_ids.contains(close_id.as_str()) {
                match orders::get_order(&state.client, close_id).await {
                    Ok(order) => match order.status.as_str() {
                        "filled" | "partially_filled" => {
                            let close_price = order.filled_avg_price
                                .as_deref()
                                .and_then(|p| p.parse::<f64>().ok())
                                .unwrap_or(0.0);

                            let pnl = (close_price - meta.entry_price)
                                * meta.qty as f64
                                * 100.0;

                            let _ = state.log_tx.send(LogEvent::order(format!(
                                "CLOSED {contract} x{} @ ${:.2}  P&L: {:+.2}",
                                meta.qty, close_price, pnl,
                            )));

                            // Write close record to DB
                            let dte = crate::exit_monitor::dte_from_occ_symbol(&contract)
                                .unwrap_or(0);
                            let _ = state.db.insert_trade_log(
                                &contract, &meta.underlying, &meta.direction,
                                "close", close_price, meta.qty as i64,
                                Some(meta.confluence_score as i64),
                                Some(meta.regime.as_str()),
                                Some(meta.iv_rank),
                                Some(meta.delta),
                                Some(dte as i64),
                                Some("fill_confirmed"),
                                Some(pnl),
                            ).await;

                            // Record day trade if applicable
                            let (is_day_trade, emergency_stop_pct) = {
                                let pdt = state.pdt.lock().await;
                                (pdt.would_be_day_trade(meta.opened_at),
                                 pdt.emergency_stop_pct)
                            };
                            if is_day_trade {
                                let pnl_pct = if meta.entry_price > 0.0 {
                                    (close_price - meta.entry_price) / meta.entry_price * 100.0
                                } else { 0.0 };
                                let dt_record = DayTradeRecord {
                                    contract_symbol: contract.clone(),
                                    underlying:      meta.underlying.clone(),
                                    open_time:       meta.opened_at,
                                    close_time:      Utc::now(),
                                    open_price:      meta.entry_price,
                                    close_price,
                                    pnl,
                                    was_emergency:   pnl_pct <= -emergency_stop_pct,
                                };
                                let _ = state.db.insert_day_trade(&dt_record).await;
                                let mut pdt = state.pdt.lock().await;
                                pdt.record(dt_record);
                            }

                            state.open_positions.lock().await.remove(&contract);

                            // Record close timestamp for entry cooldown veto
                            state.last_close_by_underlying.lock().await
                                .insert(meta.underlying.clone(), Utc::now());
                        }
                        "canceled" | "expired" => {
                            let _ = state.log_tx.send(LogEvent::warn(format!(
                                "close order {close_id} for {contract} was {} — will retry next exit check",
                                order.status
                            )));
                            // Clear pending_close so exit monitor retries
                            let mut positions = state.open_positions.lock().await;
                            if let Some(m) = positions.get_mut(&contract) {
                                m.pending_close_order_id = None;
                            }
                        }
                        other => {
                            let _ = state.log_tx.send(LogEvent::warn(format!("close order {close_id} for {contract}: unexpected status {other}")));
                        }
                    },
                    Err(e) => {
                        let _ = state.log_tx.send(LogEvent::warn(format!("could not fetch close order {close_id}: {e}")));
                    }
                }
            }
        }
    }

    Ok(())
}
