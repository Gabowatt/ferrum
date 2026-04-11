/// Pure indicator calculations on price/volume series.
/// All functions take slices of f64 (oldest first) and return computed values.

// ── EMA ───────────────────────────────────────────────────────────────────────

/// Compute EMA for the full series. Returns a vec of the same length (NaN-padded at the start).
pub fn ema(closes: &[f64], period: usize) -> Vec<f64> {
    if closes.len() < period {
        return vec![f64::NAN; closes.len()];
    }
    let k = 2.0 / (period as f64 + 1.0);
    let mut result = vec![f64::NAN; closes.len()];

    // Seed with SMA of first `period` values
    let seed: f64 = closes[..period].iter().sum::<f64>() / period as f64;
    result[period - 1] = seed;

    for i in period..closes.len() {
        result[i] = closes[i] * k + result[i - 1] * (1.0 - k);
    }
    result
}

/// Latest EMA value for a given period, or NaN if insufficient data.
pub fn ema_last(closes: &[f64], period: usize) -> f64 {
    *ema(closes, period).last().unwrap_or(&f64::NAN)
}

// ── RSI ───────────────────────────────────────────────────────────────────────

/// Wilder's RSI (14-period typical). Returns a vec the same length as closes (NaN-padded).
pub fn rsi(closes: &[f64], period: usize) -> Vec<f64> {
    if closes.len() <= period {
        return vec![f64::NAN; closes.len()];
    }

    let mut gains = vec![0.0f64; closes.len()];
    let mut losses = vec![0.0f64; closes.len()];

    for i in 1..closes.len() {
        let diff = closes[i] - closes[i - 1];
        if diff > 0.0 { gains[i] = diff; } else { losses[i] = -diff; }
    }

    let mut result = vec![f64::NAN; closes.len()];

    // Seed with simple averages
    let avg_gain: f64 = gains[1..=period].iter().sum::<f64>() / period as f64;
    let avg_loss: f64 = losses[1..=period].iter().sum::<f64>() / period as f64;

    let mut ag = avg_gain;
    let mut al = avg_loss;

    result[period] = if al == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + ag / al) };

    for i in (period + 1)..closes.len() {
        ag = (ag * (period as f64 - 1.0) + gains[i]) / period as f64;
        al = (al * (period as f64 - 1.0) + losses[i]) / period as f64;
        result[i] = if al == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + ag / al) };
    }
    result
}

pub fn rsi_last(closes: &[f64], period: usize) -> f64 {
    *rsi(closes, period).last().unwrap_or(&f64::NAN)
}

// ── MACD ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MacdResult {
    pub macd:      f64,
    pub signal:    f64,
    pub histogram: f64,
}

pub fn macd_last(closes: &[f64], fast: usize, slow: usize, signal: usize) -> MacdResult {
    let ema_fast = ema(closes, fast);
    let ema_slow = ema(closes, slow);

    let macd_line: Vec<f64> = ema_fast.iter().zip(ema_slow.iter())
        .map(|(f, s)| if f.is_nan() || s.is_nan() { f64::NAN } else { f - s })
        .collect();

    let macd_clean: Vec<f64> = macd_line.iter().filter(|v| !v.is_nan()).cloned().collect();
    let signal_line = ema(&macd_clean, signal);

    let macd_val = *macd_clean.last().unwrap_or(&f64::NAN);
    let sig_val  = *signal_line.last().unwrap_or(&f64::NAN);

    MacdResult {
        macd:      macd_val,
        signal:    sig_val,
        histogram: if macd_val.is_nan() || sig_val.is_nan() { f64::NAN } else { macd_val - sig_val },
    }
}

// ── ADX ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AdxResult {
    pub adx:      f64,
    pub plus_di:  f64,
    pub minus_di: f64,
}

pub fn adx_last(highs: &[f64], lows: &[f64], closes: &[f64], period: usize) -> AdxResult {
    let n = highs.len().min(lows.len()).min(closes.len());
    if n < period + 1 {
        return AdxResult { adx: f64::NAN, plus_di: f64::NAN, minus_di: f64::NAN };
    }

    let mut tr  = Vec::with_capacity(n);
    let mut pdm = Vec::with_capacity(n);
    let mut ndm = Vec::with_capacity(n);

    for i in 1..n {
        let h = highs[i]; let l = lows[i]; let pc = closes[i - 1];
        tr.push((h - l).max((h - pc).abs()).max((l - pc).abs()));
        let up   = h - highs[i - 1];
        let down = lows[i - 1] - l;
        if up > down && up > 0.0 { pdm.push(up); ndm.push(0.0); }
        else if down > up && down > 0.0 { pdm.push(0.0); ndm.push(down); }
        else { pdm.push(0.0); ndm.push(0.0); }
    }

    // Wilder smoothing
    let smooth = |v: &[f64]| -> Vec<f64> {
        let mut s = Vec::with_capacity(v.len());
        let seed: f64 = v[..period].iter().sum();
        s.push(seed);
        for &x in &v[period..] {
            s.push(*s.last().unwrap() - *s.last().unwrap() / period as f64 + x);
        }
        s
    };

    let str_ = smooth(&tr);
    let spd  = smooth(&pdm);
    let snd  = smooth(&ndm);

    let mut dx_vals = Vec::new();
    for i in 0..str_.len() {
        let pdi = if str_[i] > 0.0 { 100.0 * spd[i] / str_[i] } else { 0.0 };
        let ndi = if str_[i] > 0.0 { 100.0 * snd[i] / str_[i] } else { 0.0 };
        let dx  = if pdi + ndi > 0.0 { 100.0 * (pdi - ndi).abs() / (pdi + ndi) } else { 0.0 };
        dx_vals.push((pdi, ndi, dx));
    }

    let adx_seed: f64 = dx_vals[..period].iter().map(|v| v.2).sum::<f64>() / period as f64;
    let mut adx = adx_seed;
    for i in period..dx_vals.len() {
        adx = (adx * (period as f64 - 1.0) + dx_vals[i].2) / period as f64;
    }

    let last = dx_vals.last().unwrap();
    AdxResult { adx, plus_di: last.0, minus_di: last.1 }
}

// ── Bollinger Bands ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BBands {
    pub upper:  f64,
    pub middle: f64,
    pub lower:  f64,
    /// (upper - lower) / middle — as a fraction, not percent.
    pub width:  f64,
}

pub fn bbands_last(closes: &[f64], period: usize, std_dev: f64) -> BBands {
    if closes.len() < period {
        return BBands { upper: f64::NAN, middle: f64::NAN, lower: f64::NAN, width: f64::NAN };
    }
    let window = &closes[closes.len() - period..];
    let mean: f64 = window.iter().sum::<f64>() / period as f64;
    let variance: f64 = window.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / period as f64;
    let sd = variance.sqrt();
    let upper  = mean + std_dev * sd;
    let lower  = mean - std_dev * sd;
    BBands { upper, middle: mean, lower, width: (upper - lower) / mean }
}

// ── ATR ───────────────────────────────────────────────────────────────────────

pub fn atr_last(highs: &[f64], lows: &[f64], closes: &[f64], period: usize) -> f64 {
    let n = highs.len().min(lows.len()).min(closes.len());
    if n < period + 1 { return f64::NAN; }

    let trs: Vec<f64> = (1..n).map(|i| {
        let pc = closes[i - 1];
        (highs[i] - lows[i])
            .max((highs[i] - pc).abs())
            .max((lows[i] - pc).abs())
    }).collect();

    let mut atr = trs[..period].iter().sum::<f64>() / period as f64;
    for &tr in &trs[period..] {
        atr = (atr * (period as f64 - 1.0) + tr) / period as f64;
    }
    atr
}

// ── Volume ratio ──────────────────────────────────────────────────────────────

/// Current bar volume / average of last `period` bars volume.
pub fn volume_ratio(volumes: &[f64], period: usize) -> f64 {
    if volumes.len() < period + 1 { return f64::NAN; }
    let current = *volumes.last().unwrap();
    let avg: f64 = volumes[volumes.len() - 1 - period..volumes.len() - 1]
        .iter().sum::<f64>() / period as f64;
    if avg == 0.0 { f64::NAN } else { current / avg }
}

// ── Historical volatility (HV) ────────────────────────────────────────────────

/// 20-day annualised historical volatility (as a fraction, not percent).
pub fn historical_volatility(closes: &[f64], period: usize) -> f64 {
    if closes.len() < period + 1 { return f64::NAN; }
    let returns: Vec<f64> = closes.windows(2)
        .rev()
        .take(period)
        .map(|w| (w[1] / w[0]).ln())
        .collect();
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
    variance.sqrt() * (252.0f64).sqrt()
}

// ── Regime ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Regime {
    TrendingUp,
    TrendingDown,
    RangeBound,
    Choppy,
}

impl std::fmt::Display for Regime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Regime::TrendingUp   => write!(f, "trending_up"),
            Regime::TrendingDown => write!(f, "trending_down"),
            Regime::RangeBound   => write!(f, "range_bound"),
            Regime::Choppy       => write!(f, "choppy"),
        }
    }
}

// ── Bar context (v2.1) ────────────────────────────────────────────────────────

/// Extra bar-level data needed for regime-specific confluence scoring.
/// Computed in strategy.rs from the raw bars slice, passed into confluence_score.
#[derive(Debug, Clone)]
pub struct BarContext {
    /// Today's high.
    pub high:           f64,
    /// Today's low.
    pub low:            f64,
    /// Today's open.
    pub open:           f64,
    /// Low from exactly 5 bars ago — used for higher-low confirmation in trend-up.
    pub low_5b_ago:     f64,
    /// High from exactly 5 bars ago — used for lower-high confirmation in trend-down.
    pub high_5b_ago:    f64,
    /// Rolling 20-day high of the highs series.
    pub high_20d:       f64,
    /// Rolling 20-day low of the lows series.
    pub low_20d:        f64,
    /// MACD histogram from the previous bar — compared to current to detect "turning up/down".
    pub macd_hist_prev: f64,
}

// ── Indicator snapshot ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IndicatorSnapshot {
    pub close:      f64,
    pub ema9:       f64,
    pub ema20:      f64,
    pub ema50:      f64,
    /// EMA20 value `ema_slope_lookback_bars` bars ago — compare to ema20 for slope direction.
    pub ema20_prev: f64,
    pub rsi:        f64,
    pub macd:       MacdResult,
    pub adx:        AdxResult,
    pub bbands:     BBands,
    pub atr:        f64,
    pub vol_ratio:  f64,
    pub hv20:       f64,
    pub regime:     Regime,
}

pub fn compute_snapshot(
    closes:           &[f64],
    highs:            &[f64],
    lows:             &[f64],
    volumes:          &[f64],
    adx_trend:        f64,
    adx_no_trend:     f64,
    bb_width_min_pct: f64,
    slope_lookback:   usize,
) -> Option<IndicatorSnapshot> {
    if closes.len() < 60 { return None; }

    let close   = *closes.last()?;
    let ema9    = ema_last(closes, 9);
    let ema20   = ema_last(closes, 20);
    let ema50   = ema_last(closes, 50);
    let rsi_v   = rsi_last(closes, 14);
    let macd_v  = macd_last(closes, 12, 26, 9);
    let adx_v   = adx_last(highs, lows, closes, 14);
    let bb      = bbands_last(closes, 20, 2.0);
    let atr_v   = atr_last(highs, lows, closes, 14);
    let vol_r   = volume_ratio(volumes, 20);
    let hv      = historical_volatility(closes, 20);

    // EMA20 slope: value `slope_lookback` bars ago
    let ema20_series = ema(closes, 20);
    let n = ema20_series.len();
    let ema20_prev = if n > slope_lookback {
        let v = ema20_series[n - 1 - slope_lookback];
        if v.is_nan() { ema20 } else { v }
    } else {
        ema20
    };

    let regime = detect_regime(
        close, ema20, ema50,
        adx_v.adx, adx_v.plus_di, adx_v.minus_di,
        adx_trend, adx_no_trend,
        ema20 - ema20_prev,        // positive = EMA20 rising
        bb.width * 100.0,          // width in %
        bb_width_min_pct,
    );

    Some(IndicatorSnapshot {
        close, ema9, ema20, ema50, ema20_prev,
        rsi: rsi_v, macd: macd_v, adx: adx_v,
        bbands: bb, atr: atr_v, vol_ratio: vol_r, hv20: hv,
        regime,
    })
}

/// v2.1 regime classifier.
///
/// Trend requires: EMA stack + ADX ≥ trend_threshold + correct DI direction + EMA20 rising/falling.
/// Range requires: ADX below no-trend threshold + BB wide enough + price near long-term mean.
/// Everything else is Choppy.
fn detect_regime(
    close:            f64,
    ema20:            f64,
    ema50:            f64,
    adx:              f64,
    plus_di:          f64,
    minus_di:         f64,
    adx_trend:        f64,
    adx_no_trend:     f64,
    ema20_slope:      f64,   // positive = rising
    bb_width_pct:     f64,   // in percent
    bb_width_min_pct: f64,
) -> Regime {
    let is_trend_up = close > ema20
        && ema20 > ema50
        && adx >= adx_trend
        && plus_di > minus_di
        && ema20_slope > 0.0;

    let is_trend_down = close < ema20
        && ema20 < ema50
        && adx >= adx_trend
        && minus_di > plus_di
        && ema20_slope < 0.0;

    let is_range = adx < adx_no_trend
        && bb_width_pct >= bb_width_min_pct
        && (close - ema50).abs() / ema50 < 0.05;  // price within 5% of long-term mean

    if is_trend_up   { Regime::TrendingUp }
    else if is_trend_down { Regime::TrendingDown }
    else if is_range      { Regime::RangeBound }
    else                  { Regime::Choppy }
}

// ── Confluence scoring (v2.1) ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeDirection {
    Call,
    Put,
}

/// v2.1 regime-specific confluence scoring.
///
/// Returns `Some((score, max_score, direction))` when a valid setup is found.
/// Returns `None` if the regime is Choppy and `allow_choppy` is false.
///
/// ### Trend-Up / Trend-Down (max 12 pts)
/// | Signal | Pts | Condition |
/// |---|---|---|
/// | EMA9 pullback  | 3 | low <= ema9 <= high |
/// | EMA20 pullback | 2 | low <= ema20 <= high (can't stack with EMA9) |
/// | RSI buy/sell zone | 2 | 40–55 for calls; 45–60 for puts |
/// | MACD hist turning | 2 | hist > hist_prev (momentum inflecting) |
/// | Higher low / lower high | 2 | low > low_5b_ago for calls; high < high_5b_ago for puts |
/// | Volume contraction | 1 | vol_ratio < 0.9 (clean pullback, not distribution) |
/// | ADX above trend threshold | 1 | adx >= adx_trend (not awarded if already in trend-up gate) |
///
/// ### Range-Bound (max 10 pts)
/// | Signal | Pts | Condition |
/// |---|---|---|
/// | Band touch | 3 | low <= lower_band for calls; high >= upper_band for puts |
/// | RSI extreme | 3 | RSI ≤ 30 for calls; RSI ≥ 70 for puts |
/// | Reversal candle | 2 | bullish engulf-style for calls; bearish for puts |
/// | Distance from mean | 1 | close is ≥ 1.5 ATR from ema20 in the expected direction |
/// | Volume spike | 1 | vol_ratio > 1.2 (capitulation/exhaustion volume) |
///
/// ### Choppy (max 10 pts, same as Range, min 8 required)
/// Only reached when `allow_choppy = true`.
pub fn confluence_score(
    snap:        &IndicatorSnapshot,
    ctx:         &BarContext,
    allow_choppy: bool,
) -> Option<(u32, u32, TradeDirection)> {
    match snap.regime {
        Regime::TrendingUp   => Some(score_trend(snap, ctx, TradeDirection::Call)),
        Regime::TrendingDown => Some(score_trend(snap, ctx, TradeDirection::Put)),
        Regime::RangeBound   => Some(score_range(snap, ctx)),
        Regime::Choppy       => {
            if allow_choppy {
                Some(score_range(snap, ctx))
            } else {
                None
            }
        }
    }
}

fn score_trend(snap: &IndicatorSnapshot, ctx: &BarContext, dir: TradeDirection) -> (u32, u32, TradeDirection) {
    let mut score: u32 = 0;
    const MAX: u32 = 12;

    match dir {
        TradeDirection::Call => {
            // EMA9 or EMA20 pullback — mutually exclusive, EMA9 scores more
            if ctx.low <= snap.ema9 && snap.ema9 <= ctx.high {
                score += 3;
            } else if ctx.low <= snap.ema20 && snap.ema20 <= ctx.high {
                score += 2;
            }
            // RSI in buy zone (pulled back but not broken)
            if snap.rsi >= 40.0 && snap.rsi <= 55.0 { score += 2; }
            // MACD histogram inflecting upward
            if !ctx.macd_hist_prev.is_nan() && snap.macd.histogram > ctx.macd_hist_prev { score += 2; }
            // Higher low structure
            if ctx.low > ctx.low_5b_ago { score += 2; }
            // Volume contraction on pullback (clean, not distribution)
            if !snap.vol_ratio.is_nan() && snap.vol_ratio < 0.9 { score += 1; }
            // ADX above trend threshold (trend strengthening)
            if snap.adx.adx >= 22.0 { score += 1; }
        }
        TradeDirection::Put => {
            // EMA9 or EMA20 rally — mutually exclusive
            if ctx.high >= snap.ema9 && snap.ema9 >= ctx.low {
                score += 3;
            } else if ctx.high >= snap.ema20 && snap.ema20 >= ctx.low {
                score += 2;
            }
            // RSI in sell zone (rally but not recovered)
            if snap.rsi >= 45.0 && snap.rsi <= 60.0 { score += 2; }
            // MACD histogram inflecting downward
            if !ctx.macd_hist_prev.is_nan() && snap.macd.histogram < ctx.macd_hist_prev { score += 2; }
            // Lower high structure
            if ctx.high < ctx.high_5b_ago { score += 2; }
            // Volume contraction on rally
            if !snap.vol_ratio.is_nan() && snap.vol_ratio < 0.9 { score += 1; }
            // ADX above trend threshold
            if snap.adx.adx >= 22.0 { score += 1; }
        }
    }

    (score, MAX, dir)
}

fn score_range(snap: &IndicatorSnapshot, ctx: &BarContext) -> (u32, u32, TradeDirection) {
    // Direction: price proximity to bands determines which leg to buy
    let dist_lower = snap.close - snap.bbands.lower;
    let dist_upper = snap.bbands.upper - snap.close;
    let dir = if dist_lower < dist_upper { TradeDirection::Call } else { TradeDirection::Put };

    let mut score: u32 = 0;
    const MAX: u32 = 10;

    match dir {
        TradeDirection::Call => {
            // Band touch: today's low pierced or touched the lower band
            if ctx.low <= snap.bbands.lower { score += 3; }
            // RSI extreme oversold
            if snap.rsi <= 30.0 { score += 3; }
            // Reversal candle: bullish (close > open, lower wick > 50% of body)
            let body = (snap.close - ctx.open).abs();
            let lower_wick = ctx.open.min(snap.close) - ctx.low;
            if snap.close > ctx.open && body > 0.0 && lower_wick > 0.5 * body { score += 2; }
            // Distance from mean: at least 1.5 ATR below EMA20
            if !snap.atr.is_nan() && snap.atr > 0.0
               && snap.ema20 - snap.close >= 1.5 * snap.atr { score += 1; }
            // Volume spike: capitulation/exhaustion
            if !snap.vol_ratio.is_nan() && snap.vol_ratio > 1.2 { score += 1; }
        }
        TradeDirection::Put => {
            // Band touch: today's high pierced or touched the upper band
            if ctx.high >= snap.bbands.upper { score += 3; }
            // RSI extreme overbought
            if snap.rsi >= 70.0 { score += 3; }
            // Reversal candle: bearish (close < open, upper wick > 50% of body)
            let body = (snap.close - ctx.open).abs();
            let upper_wick = ctx.high - ctx.open.max(snap.close);
            if snap.close < ctx.open && body > 0.0 && upper_wick > 0.5 * body { score += 2; }
            // Distance from mean: at least 1.5 ATR above EMA20
            if !snap.atr.is_nan() && snap.atr > 0.0
               && snap.close - snap.ema20 >= 1.5 * snap.atr { score += 1; }
            // Volume spike
            if !snap.vol_ratio.is_nan() && snap.vol_ratio > 1.2 { score += 1; }
        }
    }

    (score, MAX, dir)
}
