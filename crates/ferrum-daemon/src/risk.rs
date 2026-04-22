use std::collections::HashMap;

use ferrum_core::{
    config::AppConfig,
    error::FerrumError,
    types::{OptionLeg, Signal},
};

pub struct RiskGuard<'a> {
    pub config:         &'a AppConfig,
    pub open_positions: u32,
    pub daily_pnl_pct:  f64,
    pub account_equity: f64,
    pub total_at_risk:  f64,
    /// Count of open positions by sector (derived from open underlyings + config
    /// sector map). Used to enforce `sizing.max_sector_positions`.
    pub sector_counts:  HashMap<String, u32>,
}

impl<'a> RiskGuard<'a> {
    pub fn new(
        config: &'a AppConfig,
        open_positions: u32,
        daily_pnl_pct: f64,
        account_equity: f64,
        total_at_risk: f64,
    ) -> Self {
        Self {
            config,
            open_positions,
            daily_pnl_pct,
            account_equity,
            total_at_risk,
            sector_counts: HashMap::new(),
        }
    }

    /// Populate sector counts from the list of currently-open underlyings.
    /// Call this before `check_entry` so the sector concentration check runs.
    pub fn with_open_underlyings(mut self, underlyings: &[String]) -> Self {
        let mut counts: HashMap<String, u32> = HashMap::new();
        for u in underlyings {
            let sector = self.config.symbols.sector_of(u).to_string();
            *counts.entry(sector).or_insert(0) += 1;
        }
        self.sector_counts = counts;
        self
    }

    /// Validate a potential entry signal. Returns Ok(()) or Err(RiskViolation).
    /// Live-mode gating now lives in `main.rs` via `live.enabled`; no hard
    /// block here so paper + opted-in-live both reach this check.
    pub fn check_entry(&self, signal: &Signal) -> Result<(), FerrumError> {
        // Equity floor.
        if self.account_equity < self.config.risk.halt_equity_floor {
            return Err(FerrumError::RiskViolation(format!(
                "account equity ${:.2} below halt floor ${:.2}",
                self.account_equity, self.config.risk.halt_equity_floor
            )));
        }

        // Daily drawdown.
        if self.daily_pnl_pct <= -self.config.risk.daily_drawdown_pct {
            return Err(FerrumError::RiskViolation(format!(
                "daily drawdown {:.2}% exceeds limit {:.2}%",
                self.daily_pnl_pct.abs(), self.config.risk.daily_drawdown_pct
            )));
        }

        // Max open positions.
        if self.open_positions >= self.config.sizing.max_open_positions {
            return Err(FerrumError::RiskViolation(format!(
                "max open positions {} reached", self.config.sizing.max_open_positions
            )));
        }

        // Sector concentration: block if the underlying's sector is already at
        // `max_sector_positions`.
        if let Some(underlying) = signal_underlying(signal) {
            let sector = self.config.symbols.sector_of(underlying);
            let current = self.sector_counts.get(sector).copied().unwrap_or(0);
            if current >= self.config.sizing.max_sector_positions {
                return Err(FerrumError::RiskViolation(format!(
                    "sector '{sector}' already has {current} open positions (limit {})",
                    self.config.sizing.max_sector_positions
                )));
            }
        }

        // Portfolio risk limit.
        let available = self.account_equity * (1.0 - self.config.sizing.min_cash_reserve_pct / 100.0);
        let max_risk  = available * self.config.sizing.max_portfolio_risk_pct / 100.0;
        if self.total_at_risk >= max_risk {
            return Err(FerrumError::RiskViolation(format!(
                "portfolio risk ${:.2} at limit ${:.2}", self.total_at_risk, max_risk
            )));
        }

        // Position size check.
        if let Some(cost) = estimate_position_usd(signal) {
            if cost > self.config.sizing.max_position_usd {
                return Err(FerrumError::RiskViolation(format!(
                    "position cost ${cost:.2} exceeds max ${:.2}",
                    self.config.sizing.max_position_usd
                )));
            }
            // Cash reserve check.
            let cash_after = self.account_equity - self.total_at_risk - cost;
            let min_cash   = self.account_equity * self.config.sizing.min_cash_reserve_pct / 100.0;
            if cash_after < min_cash {
                return Err(FerrumError::RiskViolation(format!(
                    "order would breach cash reserve (remaining ${cash_after:.2} < min ${min_cash:.2})"
                )));
            }
        }

        Ok(())
    }
}

fn estimate_position_usd(signal: &Signal) -> Option<f64> {
    let legs: &Vec<OptionLeg> = match signal {
        Signal::EnterLong  { legs, .. } => legs,
        Signal::EnterShort { legs, .. } => legs,
        Signal::Exit { .. }             => return None,
    };
    let total: f64 = legs.iter()
        .filter_map(|leg| leg.limit_price.map(|p| p * leg.qty as f64 * 100.0))
        .sum();
    if total > 0.0 { Some(total) } else { None }
}

fn signal_underlying(signal: &Signal) -> Option<&str> {
    match signal {
        Signal::EnterLong  { symbol, .. } => Some(symbol.as_str()),
        Signal::EnterShort { symbol, .. } => Some(symbol.as_str()),
        Signal::Exit { symbol }           => Some(symbol.as_str()),
    }
}
