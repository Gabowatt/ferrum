use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Bot status ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BotStatus {
    Idle,
    Running,
    Stopping,
}

impl std::fmt::Display for BotStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BotStatus::Idle     => write!(f, "IDLE"),
            BotStatus::Running  => write!(f, "RUNNING"),
            BotStatus::Stopping => write!(f, "STOPPING"),
        }
    }
}

// ── Log events ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Signal,
    Order,
    Risk,
    Error,
    Warn,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info   => write!(f, "INFO"),
            LogLevel::Signal => write!(f, "SIGNAL"),
            LogLevel::Order  => write!(f, "ORDER"),
            LogLevel::Risk   => write!(f, "RISK"),
            LogLevel::Error  => write!(f, "ERROR"),
            LogLevel::Warn   => write!(f, "WARN"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub timestamp: DateTime<Utc>,
    pub level:     LogLevel,
    pub message:   String,
}

impl LogEvent {
    pub fn info(msg: impl Into<String>) -> Self {
        Self { timestamp: Utc::now(), level: LogLevel::Info, message: msg.into() }
    }

    pub fn signal(msg: impl Into<String>) -> Self {
        Self { timestamp: Utc::now(), level: LogLevel::Signal, message: msg.into() }
    }

    pub fn order(msg: impl Into<String>) -> Self {
        Self { timestamp: Utc::now(), level: LogLevel::Order, message: msg.into() }
    }

    pub fn risk(msg: impl Into<String>) -> Self {
        Self { timestamp: Utc::now(), level: LogLevel::Risk, message: msg.into() }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self { timestamp: Utc::now(), level: LogLevel::Error, message: msg.into() }
    }

    pub fn warn(msg: impl Into<String>) -> Self {
        Self { timestamp: Utc::now(), level: LogLevel::Warn, message: msg.into() }
    }
}

// ── IPC protocol ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcCommand {
    Status,
    Start,
    Stop,
    ToggleMode { mode: String },
    GetPnl { period: String },
    GetEquityHistory { period: String },
    GetFills,
    GetPositions,
    GetPdt,
    GetMarketClock,
    GetLogs { limit: u32 },
    /// V2.1 Phase 2: list registered strategies + per-strategy live stats.
    GetStrategies,
    /// V2.1 Phase 2: live-toggle a strategy and persist the choice to config.toml.
    SetStrategyEnabled { id: String, enabled: bool },
    /// Live price snapshot for the configured scan universe — used by the
    /// header ticker strip. Returns price + day-change % per symbol.
    GetTickerSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    Status {
        status: BotStatus,
        mode:   String,
    },
    Ok,
    Error {
        message: String,
    },
    Pnl {
        today: f64,
        month: f64,
        year:  f64,
    },
    Fills {
        fills: Vec<FillRecord>,
    },
    Positions {
        positions: Vec<Position>,
    },
    PdtStatus {
        used: u32,
        max:  u32,
    },
    MarketClock {
        is_open:     bool,
        next_change: String,   // e.g. "opens 09:30" or "closes 16:00"
    },
    Logs {
        events: Vec<LogEvent>,
    },
    EquityHistory {
        timestamps: Vec<i64>,   // unix milliseconds
        equity:     Vec<f64>,
    },
    /// V2.1 Phase 2: list of registered strategies + live stats.
    Strategies {
        strategies: Vec<StrategyInfo>,
    },
    /// Live ticker snapshot for the scan universe (header marquee).
    TickerSnapshot {
        entries: Vec<TickerEntry>,
    },
    /// Server → client push: streamed log event
    LogEvent(LogEvent),
}

// ── Ticker snapshot entry ─────────────────────────────────────────────────────
//
// One row per scanned symbol for the header's Nasdaq-style scrolling banner.
// `change_pct` is a fraction (0.0125 = +1.25 %); the UI formats display.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickerEntry {
    pub symbol:     String,
    pub price:      f64,
    pub change_pct: f64,
}

// ── Strategy registry info (V2.1 Phase 2) ────────────────────────────────────
//
// Mirror of `daemon::strategy::StrategyHandle` enriched with per-strategy stats
// the dashboard cares about (open positions, scan tally for today). Sent in
// response to `IpcCommand::GetStrategies`.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyInfo {
    pub id:                  String,
    pub enabled:             bool,
    pub scan_interval_secs:  u64,
    /// Open positions currently attributed to this strategy.
    pub open_positions:      u32,
    /// Number of scan_results rows with `outcome = 'entered'` and matching
    /// `strategy_id` since UTC midnight.
    pub signals_today:       u32,
    /// Total scan_results rows for this strategy since UTC midnight (any outcome).
    pub scans_today:         u32,
}

// ── Position ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub contract:        String,
    pub underlying:      String,
    pub direction:       String,   // "call" or "put"
    pub qty:             f64,
    pub entry_price:     f64,
    pub current_price:   f64,
    pub market_value:    f64,
    pub unrealized_pl:   f64,
    pub unrealized_plpc: f64,   // as fraction e.g. 0.15 = 15%
    pub opened_at:       DateTime<Utc>,
    /// V2.1 Phase 2: which strategy owns this position. `None` when the position
    /// pre-dates strategy attribution (legacy DB rows) or was opened outside
    /// the registry (manual order on Alpaca side).
    #[serde(default)]
    pub strategy_id:     Option<String>,
}

// ── Fill records ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillRecord {
    pub id:        Option<i64>,
    pub symbol:    String,
    pub side:      String,
    pub qty:       f64,
    pub price:     f64,
    pub timestamp: DateTime<Utc>,
    pub order_id:  String,
}

// ── Strategy types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Signal {
    EnterLong  { symbol: String, legs: Vec<OptionLeg> },
    EnterShort { symbol: String, legs: Vec<OptionLeg> },
    Exit       { symbol: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionLeg {
    pub contract:    String,
    pub action:      LegAction,
    pub qty:         u32,
    pub order_type:  OrderType,
    pub limit_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LegAction {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderType {
    Limit,
    Market,
}
