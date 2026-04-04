use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    signal,
};
use tracing::{error, info, warn};

use ferrum_core::types::{BotStatus, IpcCommand, IpcResponse, LogEvent};

use crate::{strategy, AppState};

const SOCK_PATH: &str = "/tmp/ferrum.sock";

pub async fn run_server(state: Arc<AppState>) -> Result<(), ferrum_core::error::FerrumError> {
    // Remove stale socket if it exists.
    let _ = std::fs::remove_file(SOCK_PATH);

    let listener = UnixListener::bind(SOCK_PATH)?;
    info!("IPC listening on {SOCK_PATH}");

    // Spawn fill-sync background task.
    tokio::spawn(crate::strategy::fill_sync_task(state.clone()));

    // Graceful shutdown via SIGINT / SIGTERM.
    let state_shutdown = state.clone();
    tokio::spawn(async move {
        let mut sigint  = signal::unix::signal(signal::unix::SignalKind::interrupt()).unwrap();
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate()).unwrap();
        tokio::select! {
            _ = sigint.recv()  => {},
            _ = sigterm.recv() => {},
        }
        info!("Shutdown signal received — stopping");
        *state_shutdown.status.lock().await = BotStatus::Stopping;
        let _ = state_shutdown.log_tx.send(LogEvent::warn("daemon shutting down"));
        std::fs::remove_file(SOCK_PATH).ok();
        std::process::exit(0);
    });

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let state = state.clone();
                tokio::spawn(handle_connection(stream, state));
            }
            Err(e) => {
                error!("IPC accept error: {e}");
            }
        }
    }
}

async fn handle_connection(stream: UnixStream, state: Arc<AppState>) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let cmd: IpcCommand = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(e) => {
                let resp = IpcResponse::Error { message: format!("bad command: {e}") };
                let _ = writer.write_all(json_line(&resp).as_bytes()).await;
                continue;
            }
        };

        let response = dispatch(cmd, &state).await;
        let _ = writer.write_all(json_line(&response).as_bytes()).await;
    }
}

async fn dispatch(cmd: IpcCommand, state: &Arc<AppState>) -> IpcResponse {
    match cmd {
        IpcCommand::Status => {
            let status = *state.status.lock().await;
            IpcResponse::Status {
                status,
                mode: state.config.alpaca.mode.to_string(),
            }
        }

        IpcCommand::Start => {
            let mut s = state.status.lock().await;
            if *s == BotStatus::Running {
                return IpcResponse::Error { message: "already running".into() };
            }
            *s = BotStatus::Running;
            drop(s);
            let _ = state.log_tx.send(LogEvent::info("strategy loop started"));
            tokio::spawn(strategy::run_strategy_loop(state.clone()));
            IpcResponse::Ok
        }

        IpcCommand::Stop => {
            let mut s = state.status.lock().await;
            if *s != BotStatus::Running {
                return IpcResponse::Error { message: "not running".into() };
            }
            *s = BotStatus::Stopping;
            let _ = state.log_tx.send(LogEvent::warn("strategy loop stopping"));
            IpcResponse::Ok
        }

        IpcCommand::ToggleMode { mode } => {
            if mode != "paper" {
                warn!("Live mode toggle attempted — refused in V1");
                return IpcResponse::Error {
                    message: "live trading is disabled in V1".into(),
                };
            }
            IpcResponse::Ok
        }

        IpcCommand::GetPnl { period } => {
            match fetch_pnl(&state.client, &period).await {
                Ok(resp) => resp,
                Err(e)   => IpcResponse::Error { message: e.to_string() },
            }
        }

        IpcCommand::GetFills => {
            match state.db.recent_fills(50).await {
                Ok(fills) => IpcResponse::Fills { fills },
                Err(e)    => IpcResponse::Error { message: e.to_string() },
            }
        }
    }
}

async fn fetch_pnl(
    client: &ferrum_core::client::AlpacaClient,
    period: &str,
) -> Result<IpcResponse, ferrum_core::error::FerrumError> {
    let data: serde_json::Value = client
        .get_with_query("/v2/account/portfolio/history", &[("period", period), ("timeframe", "1D")])
        .await?;

    let profit_loss = data["profit_loss"].as_array().and_then(|v| v.last()).and_then(|v| v.as_f64()).unwrap_or(0.0);

    Ok(IpcResponse::Pnl {
        today: profit_loss,
        month: 0.0, // TODO: derive from history array
        year:  0.0,
    })
}

fn json_line(resp: &IpcResponse) -> String {
    let mut s = serde_json::to_string(resp).unwrap_or_else(|_| "{}".to_string());
    s.push('\n');
    s
}
