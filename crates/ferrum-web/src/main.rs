mod ipc;
mod routes;

use std::{sync::Arc, time::Duration};

use axum::{
    http::Method,
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing::info;

use ferrum_core::types::{IpcCommand, IpcResponse};

pub struct AppState {
    pub log_tx: broadcast::Sender<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ferrum_web=info".parse().unwrap()),
        )
        .init();

    let (log_tx, _) = broadcast::channel::<String>(1024);
    let state = Arc::new(AppState { log_tx: log_tx.clone() });

    // Background task: poll daemon logs every 2s and broadcast new events for SSE clients.
    tokio::spawn(log_poll_task(log_tx));

    // Determine web/ dist path — look for FERRUM_WEB_DIST env var, fall back to "web/dist".
    let dist_path = std::env::var("FERRUM_WEB_DIST").unwrap_or_else(|_| "web/dist".into());
    let port = std::env::var("FERRUM_WEB_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3000);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let api = Router::new()
        .route("/api/status",    get(routes::get_status))
        .route("/api/pnl",       get(routes::get_pnl))
        .route("/api/positions", get(routes::get_positions))
        .route("/api/fills",     get(routes::get_fills))
        .route("/api/pdt",       get(routes::get_pdt))
        .route("/api/clock",     get(routes::get_clock))
        .route("/api/logs",      get(routes::get_logs))
        .route("/api/equity",    get(routes::get_equity))
        .route("/api/start",     post(routes::post_start))
        .route("/api/stop",      post(routes::post_stop))
        .route("/api/mode",      post(routes::post_mode))
        .route("/api/strategies", get(routes::get_strategies))
        // NOTE: axum 0.7 uses `:id` for path params; `{id}` is a literal in 0.7
        // and only became the param syntax in 0.8. Using `{id}` silently routes
        // to the static-file fallback → 405. Don't change this without bumping axum.
        .route("/api/strategies/:id/enabled", post(routes::post_strategy_enabled))
        .route("/api/ticker",    get(routes::get_ticker))
        .route("/api/stream",    get(routes::sse_stream))
        .with_state(state);

    // Serve built React app from web/dist, with SPA fallback to index.html.
    let static_files = ServeDir::new(&dist_path)
        .append_index_html_on_directories(true);

    let app = Router::new()
        .merge(api)
        .fallback_service(static_files)
        .layer(cors);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await
        .unwrap_or_else(|e| { panic!("Failed to bind {addr}: {e}"); });

    info!("ferrum-web listening on http://{addr}  (dist: {dist_path})");
    axum::serve(listener, app).await.unwrap();
}

/// Poll daemon GetLogs every 2s, broadcast new events to all SSE subscribers.
async fn log_poll_task(log_tx: broadcast::Sender<String>) {
    let mut last_ts: Option<DateTime<Utc>> = None;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        let Some(IpcResponse::Logs { events }) =
            ipc::send_ipc(IpcCommand::GetLogs { limit: 50 }).await
        else {
            continue;
        };

        for ev in events {
            let is_new = last_ts.map(|t| ev.timestamp > t).unwrap_or(true);
            if is_new {
                last_ts = Some(ev.timestamp);
                if let Ok(json) = serde_json::to_string(&ev) {
                    let _ = log_tx.send(json);
                }
            }
        }
    }
}
