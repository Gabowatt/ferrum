use ferrum_core::{
    config::{AppConfig, Mode},
    error::FerrumError,
    types::{OptionLeg, Signal},
};

pub struct RiskGuard<'a> {
    pub config:        &'a AppConfig,
    pub open_positions: u32,
    pub daily_pnl_pct:  f64,
    pub account_equity: f64,
    pub total_at_risk:  f64,
}

impl<'a> RiskGuard<'a> {
    pub fn new(
        config: &'a AppConfig,
        open_positions: u32,
        daily_pnl_pct: f64,
        account_equity: f64,
        total_at_risk: f64,
    ) -> Self {
        Self { config, open_positions, daily_pnl_pct, account_equity, total_at_risk }
    }

    /// Validate a potential entry signal. Returns Ok(()) or Err(RiskViolation).
    pub fn check_entry(&self, signal: &Signal) -> Result<(), FerrumError> {
        // Hard block on live trading in V1.
        if self.config.alpaca.mode == Mode::Live {
            return Err(FerrumError::LiveTradingDisabled);
        }

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

        // Portfolio risk limit.
        let available = self.account_equity * (1.0 - self.config.sizing.min_cash_reserve_pct / 100.0);
        let max_risk  = available * self.config.sizing.max_portfolio_risk_pct / 100.0;
        if self.total_at_risk >= max_risk {
            return Err(FerrumError::RiskViolation(format!(
                "portfolio risk ${:.2} at limit ${:.2}", self.total_at_risk, max_risk
            )));
        }

        // Position size check.
        if let Some(cost) = self.estimate_position_usd(signal) {
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

    fn estimate_position_usd(&self, signal: &Signal) -> Option<f64> {
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
}
