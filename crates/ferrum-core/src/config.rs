use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub alpaca:   AlpacaConfig,
    pub polygon:  PolygonConfig,
    pub risk:     RiskConfig,
    pub strategy: StrategyConfig,
}

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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolygonConfig {
    pub key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RiskConfig {
    pub max_position_usd:   f64,
    pub daily_drawdown_pct: f64,
    pub max_open_legs:      u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StrategyConfig {
    pub symbols:            Vec<String>,
    pub scan_interval_secs: u64,
    pub delta_scan:         Option<DeltaScanConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeltaScanConfig {
    pub delta_min:   f64,
    pub delta_max:   f64,
    pub iv_rank_min: f64,
    pub dte_min:     u32,
    pub dte_max:     u32,
}

impl AppConfig {
    pub fn load(path: &str) -> Result<Self, crate::error::FerrumError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| crate::error::FerrumError::Config(format!("cannot read {path}: {e}")))?;
        toml::from_str(&contents)
            .map_err(|e| crate::error::FerrumError::Config(format!("parse error in {path}: {e}")))
    }

    /// Returns the active Alpaca base URL and credentials for the configured mode.
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
