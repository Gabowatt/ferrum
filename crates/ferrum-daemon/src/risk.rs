use ferrum_core::{
    config::{AppConfig, Mode},
    error::FerrumError,
    types::{OptionLeg, Signal},
};

pub struct RiskGuard<'a> {
    pub config: &'a AppConfig,
    pub open_legs: u32,
    pub daily_pnl_pct: f64,
}

impl<'a> RiskGuard<'a> {
    pub fn new(config: &'a AppConfig, open_legs: u32, daily_pnl_pct: f64) -> Self {
        Self { config, open_legs, daily_pnl_pct }
    }

    /// Validate a signal before order submission. Returns Ok(()) or Err(RiskViolation).
    pub fn check_signal(&self, signal: &Signal) -> Result<(), FerrumError> {
        // Hard block on live trading in V1.
        if self.config.alpaca.mode == Mode::Live {
            return Err(FerrumError::LiveTradingDisabled);
        }

        // Daily drawdown check.
        if self.daily_pnl_pct <= -self.config.risk.daily_drawdown_pct {
            return Err(FerrumError::RiskViolation(format!(
                "daily drawdown limit reached ({:.2}% <= -{:.2}%)",
                self.daily_pnl_pct, self.config.risk.daily_drawdown_pct
            )));
        }

        // Max open legs check.
        let new_legs = match signal {
            Signal::EnterLong  { legs, .. } => legs.len() as u32,
            Signal::EnterShort { legs, .. } => legs.len() as u32,
            Signal::Exit { .. } => 0,
        };
        if self.open_legs + new_legs > self.config.risk.max_open_legs {
            return Err(FerrumError::RiskViolation(format!(
                "max open legs exceeded ({} + {} > {})",
                self.open_legs, new_legs, self.config.risk.max_open_legs
            )));
        }

        // Position size check (estimate from limit prices).
        if let Some(total_usd) = self.estimate_position_usd(signal) {
            if total_usd > self.config.risk.max_position_usd {
                return Err(FerrumError::RiskViolation(format!(
                    "position size ${total_usd:.2} exceeds max ${:.2}",
                    self.config.risk.max_position_usd
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

        let total: f64 = legs.iter().filter_map(|leg| {
            leg.limit_price.map(|p| p * leg.qty as f64 * 100.0) // options are 100x multiplier
        }).sum();

        if total > 0.0 { Some(total) } else { None }
    }
}
