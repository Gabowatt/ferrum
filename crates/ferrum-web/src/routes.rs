use std::{convert::Infallible, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        Json,
    },
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_stream::wrappers::BroadcastStream;

use ferrum_core::types::{IpcCommand, IpcResponse};

use crate::{ipc::send_ipc, AppState};

type Api = (StatusCode, Json<Value>);

fn ok(v: Value) -> Api { (StatusCode::OK, Json(v)) }
fn unavailable()  -> Api { (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "daemon unavailable"}))) }
fn bad(msg: &str) -> Api { (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": msg}))) }

// ── GET /api/status ───────────────────────────────────────────────────────────

pub async fn get_status(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::Status).await {
        Some(IpcResponse::Status { status, mode }) => ok(json!({ "status": status, "mode": mode })),
        _ => unavailable(),
    }
}

// ── GET /api/pnl ──────────────────────────────────────────────────────────────

pub async fn get_pnl(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::GetPnl { period: "1M".into() }).await {
        Some(IpcResponse::Pnl { today, month, year }) => ok(json!({ "today": today, "month": month, "year": year })),
        _ => unavailable(),
    }
}

// ── GET /api/positions ────────────────────────────────────────────────────────

pub async fn get_positions(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::GetPositions).await {
        Some(IpcResponse::Positions { positions }) => ok(serde_json::to_value(positions).unwrap_or(json!([]))),
        _ => unavailable(),
    }
}

// ── GET /api/fills ────────────────────────────────────────────────────────────

pub async fn get_fills(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::GetFills).await {
        Some(IpcResponse::Fills { fills }) => ok(serde_json::to_value(fills).unwrap_or(json!([]))),
        _ => unavailable(),
    }
}

// ── GET /api/pdt ──────────────────────────────────────────────────────────────

pub async fn get_pdt(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::GetPdt).await {
        Some(IpcResponse::PdtStatus { used, max }) => ok(json!({ "used": used, "max": max })),
        _ => unavailable(),
    }
}

// ── GET /api/clock ────────────────────────────────────────────────────────────

pub async fn get_clock(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::GetMarketClock).await {
        Some(IpcResponse::MarketClock { is_open, next_change }) => ok(json!({ "is_open": is_open, "next_change": next_change })),
        _ => unavailable(),
    }
}

// ── GET /api/logs?limit=N ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LogsQuery { limit: Option<u32> }

pub async fn get_logs(State(_s): State<Arc<AppState>>, Query(q): Query<LogsQuery>) -> Api {
    let limit = q.limit.unwrap_or(200).min(500);
    match send_ipc(IpcCommand::GetLogs { limit }).await {
        Some(IpcResponse::Logs { events }) => ok(serde_json::to_value(events).unwrap_or(json!([]))),
        _ => unavailable(),
    }
}

// ── GET /api/equity?period=1M ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct EquityQuery { period: Option<String> }

pub async fn get_equity(State(_s): State<Arc<AppState>>, Query(q): Query<EquityQuery>) -> Api {
    let period = q.period.unwrap_or_else(|| "1M".into());
    match send_ipc(IpcCommand::GetEquityHistory { period }).await {
        Some(IpcResponse::EquityHistory { timestamps, equity }) => {
            ok(json!({ "timestamps": timestamps, "equity": equity }))
        }
        _ => unavailable(),
    }
}

// ── POST /api/start ───────────────────────────────────────────────────────────

pub async fn post_start(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::Start).await {
        Some(IpcResponse::Ok)                  => ok(json!({ "ok": true })),
        Some(IpcResponse::Error { message })   => bad(&message),
        _                                       => unavailable(),
    }
}

// ── POST /api/stop ────────────────────────────────────────────────────────────

pub async fn post_stop(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::Stop).await {
        Some(IpcResponse::Ok)                  => ok(json!({ "ok": true })),
        Some(IpcResponse::Error { message })   => bad(&message),
        _                                       => unavailable(),
    }
}

// ── GET /api/strategies ───────────────────────────────────────────────────────

pub async fn get_strategies(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::GetStrategies).await {
        Some(IpcResponse::Strategies { strategies }) => {
            ok(serde_json::to_value(strategies).unwrap_or(json!([])))
        }
        _ => unavailable(),
    }
}

// ── GET /api/ticker ───────────────────────────────────────────────────────────

pub async fn get_ticker(State(_s): State<Arc<AppState>>) -> Api {
    match send_ipc(IpcCommand::GetTickerSnapshot).await {
        Some(IpcResponse::TickerSnapshot { entries }) => {
            ok(serde_json::to_value(entries).unwrap_or(json!([])))
        }
        Some(IpcResponse::Error { message }) => bad(&message),
        _ => unavailable(),
    }
}

// ── POST /api/strategies/:id/enabled ──────────────────────────────────────────
//
// Body: { "enabled": true | false }. Daemon flips the live AtomicBool and
// rewrites `[strategies.<id>].enabled` in config.toml.

#[derive(Deserialize)]
pub struct StrategyEnabledBody { enabled: bool }

pub async fn post_strategy_enabled(
    State(_s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<StrategyEnabledBody>,
) -> Api {
    match send_ipc(IpcCommand::SetStrategyEnabled { id, enabled: body.enabled }).await {
        Some(IpcResponse::Ok) => ok(json!({ "ok": true })),
        Some(IpcResponse::Error { message }) => bad(&message),
        _ => unavailable(),
    }
}

// ── POST /api/mode ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ModeBody { mode: String }

pub async fn post_mode(
    State(_s): State<Arc<AppState>>,
    Json(body): Json<ModeBody>,
) -> Api {
    if body.mode != "paper" && body.mode != "live" {
        return bad("mode must be 'paper' or 'live'");
    }
    match send_ipc(IpcCommand::ToggleMode { mode: body.mode }).await {
        Some(IpcResponse::Ok) => ok(json!({ "ok": true, "restart_required": true })),
        Some(IpcResponse::Error { message }) => bad(&message),
        _ => unavailable(),
    }
}

// ── GET /api/stream  (SSE) ────────────────────────────────────────────────────

pub async fn sse_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.log_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| async move {
        result.ok().map(|json| Ok(Event::default().data(json)))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
