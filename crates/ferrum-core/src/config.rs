use serde::{Deserialize, Serialize};

// ── Top-level ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub alpaca:    AlpacaConfig,
    pub symbols:   SymbolsConfig,
    pub liquidity: LiquidityConfig,
    pub strategy:  StrategyConfig,
    pub iv_engine: IvEngineConfig,
    pub sizing:    SizingConfig,
    pub risk:      RiskConfig,
    pub pdt:       PdtConfig,
}

// ── Alpaca ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlpacaConfig {
    pub mode:  Mode,
    pub paper: AlpacaCredentials,
    pub live:  AlpacaLiveCredentials,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Paper,
    Live,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Paper => write!(f, "PAPER"),
            Mode::Live  => write!(f, "LIVE"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlpacaCredentials {
    pub key:      String,
    pub secret:   String,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlpacaLiveCredentials {
    pub key:      String,
    pub secret:   String,
    pub base_url: String,
    pub enabled:  bool,
}

// ── Symbol universe ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SymbolsConfig {
    pub tier1:             Vec<String>,
    pub tier2:             Vec<String>,
    pub tier3:             Vec<String>,
    pub tier3_iv_rank_min: f64,
}

impl SymbolsConfig {
    /// All symbols across all tiers.
    pub fn all(&self) -> Vec<&str> {
        self.tier1.iter()
            .chain(self.tier2.iter())
            .chain(self.tier3.iter())
            .map(|s| s.as_str())
            .collect()
    }

    /// Returns the tier (1/2/3) for a symbol, or None if not found.
    pub fn tier_of(&self, symbol: &str) -> Option<u8> {
        if self.tier1.iter().any(|s| s == symbol) { return Some(1); }
        if self.tier2.iter().any(|s| s == symbol) { return Some(2); }
        if self.tier3.iter().any(|s| s == symbol) { return Some(3); }
        None
    }
}

// ── Liquidity filters ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LiquidityConfig {
    pub min_open_interest:  u32,
    pub min_daily_volume:   u32,
    pub max_bid_ask_spread: f64,
}

// ── Strategy ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StrategyConfig {
    pub name:                  String,
    pub scan_interval_secs:    u64,
    pub chain_scan_interval:   u64,
    pub exit_check_interval:   u64,
    pub scan_start_time:       String, // "HH:MM" ET
    pub scan_end_time:         String, // "HH:MM" ET
    pub market_data_cooldown:  u64,    // seconds between API calls
    pub entry:                 EntryConfig,
    pub exit:                  ExitConfig,
    pub regime:                RegimeConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EntryConfig {
    pub min_confluence_score: u32,
    pub preferred_delta:      f64,
    pub delta_min:            f64,
    pub delta_max:            f64,
    pub dte_min:              u32,
    pub dte_max:              u32,
    pub order_type:           String,
    pub limit_price_method:   String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExitConfig {
    // ── Trailing profit target ────────────────────────────────────────────────
    /// Once unrealized P&L reaches this %, begin tracking the peak and trailing.
    pub trailing_activation_pct:  f64,
    /// Close if current P&L falls this many points below the observed peak P&L.
    /// E.g. peak=35%, gap=7% → close trigger at 28%.
    pub trailing_trail_gap_pct:   f64,
    // ── Legacy fixed targets (kept for reference; trailing takes precedence) ──
    pub profit_target_partial_pct: f64,
    pub profit_target_full_pct:    f64,
    // ── Other exits ───────────────────────────────────────────────────────────
    pub stop_loss_pct:             f64,
    pub emergency_stop_pct:        f64,
    pub time_exit_dte:             u32,
    pub theta_exit_dte:            u32,
    pub theta_exit_min_pnl_pct:    f64,
    pub dead_money_days:           u32,
    pub dead_money_min_pct:        f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegimeConfig {
    pub ema_fast:               u32,
    pub ema_mid:                u32,
    pub ema_slow:               u32,
    pub adx_period:             u32,
    pub adx_trend_threshold:    f64,
    pub adx_no_trend_threshold: f64,
    pub rsi_period:             u32,
    pub rsi_overbought:         f64,
    pub rsi_oversold:           f64,
    pub macd_fast:              u32,
    pub macd_slow:              u32,
    pub macd_signal:            u32,
    pub bbands_period:          u32,
    pub bbands_std_dev:         f64,
    pub atr_period:             u32,
    pub volume_ma_period:       u32,
}

// ── IV rank engine ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IvEngineConfig {
    pub iv_rank_buy_max:        f64,
    pub iv_rank_sweet_min:      f64,
    pub iv_rank_sweet_max:      f64,
    pub iv_rank_caution_min:    f64,
    pub iv_rank_caution_factor: f64,
    pub hv_lookback_days:       u32,
}

// ── Position sizing ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SizingConfig {
    pub max_risk_per_trade_pct: f64,
    pub max_position_usd:       f64,
    pub max_portfolio_risk_pct: f64,
    pub max_open_positions:     u32,
    pub min_cash_reserve_pct:   f64,
    pub max_sector_positions:   u32,
    pub tiers:                  Vec<SizingTier>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SizingTier {
    pub score_min:   u32,
    pub score_max:   u32,
    pub size_factor: f64,
}

impl SizingConfig {
    /// Returns the size factor (0.5 / 0.75 / 1.0) for a given confluence score.
    pub fn size_factor_for(&self, score: u32) -> f64 {
        for tier in &self.tiers {
            if score >= tier.score_min && score <= tier.score_max {
                return tier.size_factor;
            }
        }
        // Default to minimum if score exceeds all tiers
        self.tiers.last().map(|t| t.size_factor).unwrap_or(0.5)
    }
}

// ── Risk guard ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RiskConfig {
    pub daily_drawdown_pct:  f64,
    pub halt_equity_floor:   f64,
    pub price_sanity_pct:    f64,
}

// ── PDT protection ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PdtConfig {
    pub max_day_trades_per_5d: u32,
    pub rolling_window_days:   u32,
    pub emergency_stop_pct:    f64,
    pub exceptional_win_pct:   f64,
    pub block_on_limit:        bool,
}

// ── Loader ────────────────────────────────────────────────────────────────────

impl AppConfig {
    pub fn load(path: &str) -> Result<Self, crate::error::FerrumError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| crate::error::FerrumError::Config(format!("cannot read {path}: {e}")))?;
        toml::from_str(&contents)
            .map_err(|e| crate::error::FerrumError::Config(format!("parse error in {path}: {e}")))
    }

    pub fn active_base_url(&self) -> &str {
        match self.alpaca.mode {
            Mode::Paper => &self.alpaca.paper.base_url,
            Mode::Live  => &self.alpaca.live.base_url,
        }
    }

    pub fn active_key(&self) -> &str {
        match self.alpaca.mode {
            Mode::Paper => &self.alpaca.paper.key,
            Mode::Live  => &self.alpaca.live.key,
        }
    }

    pub fn active_secret(&self) -> &str {
        match self.alpaca.mode {
            Mode::Paper => &self.alpaca.paper.secret,
            Mode::Live  => &self.alpaca.live.secret,
        }
    }
}
