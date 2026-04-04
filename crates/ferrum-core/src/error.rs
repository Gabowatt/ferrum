use thiserror::Error;

#[derive(Debug, Error)]
pub enum FerrumError {
    #[error("config error: {0}")]
    Config(String),

    #[error("alpaca API error: {0}")]
    Alpaca(String),

    #[error("polygon API error: {0}")]
    Polygon(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("risk violation: {0}")]
    RiskViolation(String),

    #[error("live trading disabled in v1")]
    LiveTradingDisabled,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}
