use ferrum_core::{error::FerrumError, indicators::historical_volatility};
use crate::db::Database;

/// Computes IV rank for a symbol.
/// Phase 1: uses historical volatility as a proxy until 30+ days of live IV snapshots exist.
/// Phase 2: once enough snapshots are stored, switches to actual IV rank.
#[derive(Debug, Clone)]
pub struct IvRankEngine {
    pub iv_rank_buy_max:        f64,
    pub iv_rank_caution_min:    f64,
    pub iv_rank_caution_factor: f64,
    pub hv_lookback_days:       u32,
}

#[derive(Debug, Clone)]
pub struct IvRankResult {
    pub iv_rank:    f64,  // 0–100
    pub current_iv: f64,  // raw IV from snapshot
    pub method:     IvMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IvMethod {
    /// Using HV as proxy (< 30 snapshots stored)
    HvProxy,
    /// Using actual stored IV snapshots
    ActualIv,
}

impl IvRankEngine {
    pub fn new(
        iv_rank_buy_max: f64,
        iv_rank_caution_min: f64,
        iv_rank_caution_factor: f64,
        hv_lookback_days: u32,
    ) -> Self {
        Self { iv_rank_buy_max, iv_rank_caution_min, iv_rank_caution_factor, hv_lookback_days }
    }

    /// Compute IV rank for a symbol given its current IV and daily closes.
    pub async fn compute(
        &self,
        symbol: &str,
        current_iv: f64,
        closes: &[f64],
        db: &Database,
    ) -> Result<IvRankResult, FerrumError> {
        // Try to use stored IV snapshots first.
        let snapshot_count = db.count_iv_snapshots(symbol).await?;

        if snapshot_count >= 30 {
            let (iv_low, iv_high) = db.iv_range_52w(symbol).await?;
            if iv_high > iv_low {
                let rank = (current_iv - iv_low) / (iv_high - iv_low) * 100.0;
                return Ok(IvRankResult {
                    iv_rank:    rank.clamp(0.0, 100.0),
                    current_iv,
                    method:     IvMethod::ActualIv,
                });
            }
        }

        // Fall back to HV proxy.
        let hv = historical_volatility(closes, self.hv_lookback_days as usize);
        if hv.is_nan() || closes.len() < 60 {
            // Not enough data — return neutral rank (50)
            return Ok(IvRankResult { iv_rank: 50.0, current_iv, method: IvMethod::HvProxy });
        }

        // Use close-based rolling window for HV high/low as proxy for IV range
        let window = self.hv_lookback_days as usize;
        let mut hv_vals = Vec::new();
        for i in 0..(closes.len().saturating_sub(window + 1)) {
            let slice = &closes[i..i + window + 1];
            let h = historical_volatility(slice, window);
            if !h.is_nan() { hv_vals.push(h); }
        }

        if hv_vals.is_empty() {
            return Ok(IvRankResult { iv_rank: 50.0, current_iv, method: IvMethod::HvProxy });
        }

        let hv_low  = hv_vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let hv_high = hv_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        let rank = if hv_high > hv_low {
            ((hv - hv_low) / (hv_high - hv_low) * 100.0).clamp(0.0, 100.0)
        } else {
            50.0
        };

        Ok(IvRankResult { iv_rank: rank, current_iv, method: IvMethod::HvProxy })
    }

    /// Returns the size adjustment factor based on IV rank.
    /// 1.0 in sweet spot, 0.75 in caution zone, 0.0 if above buy_max (block).
    pub fn size_factor(&self, iv_rank: f64) -> f64 {
        if iv_rank > self.iv_rank_buy_max { return 0.0; }
        if iv_rank >= self.iv_rank_caution_min { return self.iv_rank_caution_factor; }
        1.0
    }

    /// True if IV rank is in the valid buy zone.
    pub fn is_buyable(&self, iv_rank: f64) -> bool {
        iv_rank <= self.iv_rank_buy_max
    }
}
