use chrono::{DateTime, Utc};
use ferrum_core::error::FerrumError;
use crate::db::Database;

/// Pattern Day Trader tracker.
/// Counts same-day open+close trades within a rolling 5-business-day window.
#[derive(Debug, Clone)]
pub struct PdtTracker {
    pub day_trades:          Vec<DayTradeRecord>,
    pub max_per_window:      u32,
    pub rolling_window_days: u32,
    pub emergency_stop_pct:  f64,
    pub exceptional_win_pct: f64,
}

#[derive(Debug, Clone)]
pub struct DayTradeRecord {
    pub contract_symbol: String,
    pub underlying:      String,
    pub open_time:       DateTime<Utc>,
    pub close_time:      DateTime<Utc>,
    pub open_price:      f64,
    pub close_price:     f64,
    pub pnl:             f64,
    pub was_emergency:   bool,
}

impl PdtTracker {
    pub fn new(
        max_per_window: u32,
        rolling_window_days: u32,
        emergency_stop_pct: f64,
        exceptional_win_pct: f64,
    ) -> Self {
        Self {
            day_trades: Vec::new(),
            max_per_window,
            rolling_window_days,
            emergency_stop_pct,
            exceptional_win_pct,
        }
    }

    /// Load historical day trades from DB on startup.
    pub async fn load_from_db(&mut self, db: &Database) -> Result<(), FerrumError> {
        self.day_trades = db.recent_day_trades(self.rolling_window_days as i64 * 2).await?;
        Ok(())
    }

    /// Count day trades in the rolling window.
    pub fn count_in_window(&self) -> u32 {
        let cutoff = self.window_cutoff();
        self.day_trades.iter()
            .filter(|dt| dt.close_time >= cutoff)
            .count() as u32
    }

    /// True if another day trade can be made.
    pub fn can_day_trade(&self) -> bool {
        self.count_in_window() < self.max_per_window
    }

    /// True if a position would be a day trade (opened today).
    pub fn would_be_day_trade(&self, opened_at: DateTime<Utc>) -> bool {
        opened_at.date_naive() == Utc::now().date_naive()
    }

    /// Check if an exit is allowed given PDT constraints.
    /// `pnl_pct` is positive for gains, negative for losses (as % of premium paid).
    /// Returns Ok(()) or Err with explanation.
    pub fn check_exit_allowed(
        &self,
        opened_at: DateTime<Utc>,
        pnl_pct: f64,
    ) -> Result<(), String> {
        if !self.would_be_day_trade(opened_at) {
            return Ok(()); // overnight hold — not a day trade, always allowed
        }

        if self.can_day_trade() {
            return Ok(()); // still have day trade budget
        }

        // PDT limit reached — check exceptions
        let loss_pct = -pnl_pct; // positive = loss
        if loss_pct >= self.emergency_stop_pct {
            return Ok(()); // emergency stop-loss
        }

        if pnl_pct >= self.exceptional_win_pct {
            return Ok(()); // exceptional gain — take the money
        }

        Err(format!(
            "PDT limit ({}/{}) reached — holding overnight \
             (pnl {:.1}% not emergency loss ≥{:.0}% or exceptional win ≥{:.0}%)",
            self.count_in_window(), self.max_per_window,
            pnl_pct, self.emergency_stop_pct, self.exceptional_win_pct
        ))
    }

    /// Record a completed day trade.
    pub fn record(&mut self, trade: DayTradeRecord) {
        self.day_trades.push(trade);
    }

    fn window_cutoff(&self) -> DateTime<Utc> {
        // Approximate: subtract calendar days (good enough for paper trading)
        Utc::now() - chrono::Duration::days(self.rolling_window_days as i64)
    }
}
