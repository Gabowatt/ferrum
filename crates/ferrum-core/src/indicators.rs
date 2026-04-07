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
    pub adx:  f64,
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

#[derive(Debug, Clone)]
pub struct IndicatorSnapshot {
    pub close:     f64,
    pub ema9:      f64,
    pub ema20:     f64,
    pub ema50:     f64,
    pub rsi:       f64,
    pub macd:      MacdResult,
    pub adx:       AdxResult,
    pub bbands:    BBands,
    pub atr:       f64,
    pub vol_ratio: f64,
    pub hv20:      f64,
    pub regime:    Regime,
}

pub fn compute_snapshot(
    closes:  &[f64],
    highs:   &[f64],
    lows:    &[f64],
    volumes: &[f64],
    adx_trend:    f64,
    adx_no_trend: f64,
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

    let regime = detect_regime(close, ema20, ema50, adx_v.adx, adx_trend, adx_no_trend);

    Some(IndicatorSnapshot {
        close, ema9, ema20, ema50,
        rsi: rsi_v, macd: macd_v, adx: adx_v,
        bbands: bb, atr: atr_v, vol_ratio: vol_r, hv20: hv,
        regime,
    })
}

fn detect_regime(
    close: f64, ema20: f64, ema50: f64,
    adx: f64,
    adx_trend: f64, adx_no_trend: f64,
) -> Regime {
    if close > ema20 && ema20 > ema50 && adx > adx_trend {
        Regime::TrendingUp
    } else if close < ema20 && ema20 < ema50 && adx > adx_trend {
        Regime::TrendingDown
    } else if adx < adx_no_trend {
        Regime::RangeBound
    } else {
        Regime::Choppy
    }
}

// ── Confluence scoring ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeDirection {
    Call,
    Put,
}

/// Compute confluence score and return (score, direction).
///
/// Scoring rationale (max 15 pts):
///   EMA regime alignment    +3  — price above/below stacked EMAs
///   EMA proximity           +2  — within 1.5% of EMA9 or EMA20 (pullback/bounce zone)
///   RSI zone                +2  — calls 30–55, puts 45–70 (wider to survive volatile regimes)
///   MACD cross              +2  — macd vs signal line direction
///   MACD histogram          +1  — histogram sign confirmation
///   ADX momentum            +2  — ADX > 20 (reduced from 25; any meaningful trend qualifies)
///   BBand extreme           +2  — within 2% of upper/lower band
///   Volume confirmation     +1  — current bar volume ≥ 20-day average
///
/// Choppy regime: iron condors work well in range-bound/choppy markets, so
/// Choppy is treated identically to RangeBound (BBand position picks direction).
/// Only an unusually low score (< min_confluence_score) blocks entry.
pub fn confluence_score(snap: &IndicatorSnapshot, _rsi_ob: f64, _rsi_os: f64) -> Option<(u32, TradeDirection)> {
    let direction = match snap.regime {
        Regime::TrendingUp   => TradeDirection::Call,
        Regime::TrendingDown => TradeDirection::Put,
        // Range-bound and choppy: pick direction from BBand proximity.
        // Iron condors thrive when price oscillates inside a band.
        Regime::RangeBound | Regime::Choppy => {
            let dist_lower = (snap.close - snap.bbands.lower).abs();
            let dist_upper = (snap.close - snap.bbands.upper).abs();
            if dist_lower < dist_upper { TradeDirection::Call } else { TradeDirection::Put }
        }
    };

    let mut score: u32 = 0;

    match direction {
        TradeDirection::Call => {
            // EMA regime alignment (stacked bullish)
            if snap.close > snap.ema20 && snap.ema20 > snap.ema50 { score += 3; }
            // Pullback / proximity to EMA9 or EMA20 (1.5% tolerance)
            if (snap.close - snap.ema9).abs() / snap.close < 0.015
            || (snap.close - snap.ema20).abs() / snap.close < 0.015 { score += 2; }
            // RSI oversold-to-neutral zone (30–55 covers bounce + early uptrend)
            if snap.rsi >= 30.0 && snap.rsi <= 55.0 { score += 2; }
            // MACD crossover bullish
            if snap.macd.macd > snap.macd.signal { score += 2; }
            if snap.macd.histogram > 0.0 { score += 1; }
            // ADX momentum present (any directional move ≥ 20)
            if snap.adx.adx >= 20.0 { score += 2; }
            // Price near lower BBand (≤ 2% above lower band)
            if snap.close <= snap.bbands.lower * 1.02 { score += 2; }
        }
        TradeDirection::Put => {
            // EMA regime alignment (stacked bearish)
            if snap.close < snap.ema20 && snap.ema20 < snap.ema50 { score += 3; }
            // Pullback / proximity to EMA9 or EMA20 (1.5% tolerance)
            if (snap.close - snap.ema9).abs() / snap.close < 0.015
            || (snap.close - snap.ema20).abs() / snap.close < 0.015 { score += 2; }
            // RSI overbought-to-neutral zone (45–70 covers rollover + early downtrend)
            if snap.rsi >= 45.0 && snap.rsi <= 70.0 { score += 2; }
            // MACD crossover bearish
            if snap.macd.macd < snap.macd.signal { score += 2; }
            if snap.macd.histogram < 0.0 { score += 1; }
            // ADX momentum present
            if snap.adx.adx >= 20.0 { score += 2; }
            // Price near upper BBand (≥ 2% below upper band)
            if snap.close >= snap.bbands.upper * 0.98 { score += 2; }
        }
    }

    // Volume confirmation: current bar ≥ 20-day average
    if !snap.vol_ratio.is_nan() && snap.vol_ratio >= 1.0 { score += 1; }

    Some((score, direction))
}
