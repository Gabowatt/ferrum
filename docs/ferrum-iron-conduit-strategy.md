# Ferrum Trading Strategy: Iron Conduit

> **Version:** 2.2  
> **Codename:** `iron-conduit`  
> **Account Size:** $1,000  
> **Account Type:** Cash (avoids margin requirements, simplifies PDT handling)  
> **Platform:** Alpaca Trading API (paper → live via Algo Trader Plus)  
> **Asset Class:** US equity & ETF options (American-style)

## Changelog

### v2.2 (post Week 2 paper trading review — 2026-04-20)
Week 2 ran with the v2.1 vetoes in place. Results were profitable (+$78 realized over 5 sessions) but the entry rate dropped to **0.34%** (23 entries / 6,856 scans) — well under the v2.1 prediction of 3–8% and below the level needed to compound the account meaningfully.

The single biggest leak was the **trend score floor of 7**. The best trending symbol of the week (HOOD trending_down) averaged 6.16 — close, but never crossed. The whole bucket of trending_up signals on IWM/QQQ/SPY landed in the 4–6 range. Lowering the trend floor to **6/12** restores the option of taking modest-strength trend setups while keeping the regime gate strict.

Symbol pruning: PFE (range_bound avg 0.8) and PLTR (range_bound avg 0.31) never produced a usable signal in their non-choppy regime. Removed.

API rate limits: account upgraded to Alpaca Algo Trader Plus (10k req/min). Per-symbol scan throttle removed (`market_data_cooldown = 0`); TUI poll intervals dropped accordingly.

Sections updated: §3 (sizing/threshold table), §5 (Stage 3 thresholds), §13 (config reference). All other v2.1 logic (vetoes, regime gate, regime-specific signal sets, exit ladder) unchanged.

### v2.1 (post Week 1 paper trading review)
Week 1 surfaced three structural problems that the v2.0 confluence design didn't anticipate:

1. **Score compression.** Many symbols posted flat scores (CLF, BAC, SPY, QQQ all averaged exactly 6.0 across ~150 scans). A score that never moves isn't ranking anything — it's a constant. The original additive scoring let weak signals stack up to a passing total without any single signal being strong.
2. **Choppy regime dominated (69% of scans).** The original strategy explicitly forbade trading in choppy regime, then the live config allowed it as a workaround for getting any fills. Iron condors / mean-reversion *can* work in choppy markets, but the entry signals used were trend-following signals applied to a non-trending tape. CLF and NIO were stopped out within minutes because there was no trend to ride.
3. **Entries had no minimum hold protection.** A position that goes underwater in 5 minutes is almost certainly noise, not a thesis violation. Stop-losses fired before the trade had any chance.

This revision rebuilds the confluence system around three principles: **multiplicative gating** (one veto kills the trade), **regime-specific signal sets** (trend signals for trends, mean-reversion signals for ranges), and **entry quality filtering** (minimum hold time, fresh-bar gating, no entries within 1 ATR of recent extremes against you).

The intent is fewer entries, higher quality, scores that actually vary across symbols, and an honest answer to "what do we do in choppy markets?" (Answer: a *real* mean-reversion sub-strategy with tighter risk, or stand aside.)

Sections rewritten: §3 (Strategy Overview), §5 (Entry Logic — full rewrite), §6 (Exit Logic — minimum hold + market hours gating), §7 (Sizing — score-tier recalibration), §13 (Config Reference). New §5a (Regime-Specific Signal Sets). All other sections unchanged from v2.0.

### v2.0
Initial release.

---

## Table of Contents

1. PDT Rule Compliance & Account Structure
2. Symbol Universe & Budget Constraints
3. Strategy Overview — Multi-Regime Swing Trading
4. Technical Indicator Stack
5. Entry Logic — The Confluence Gate
6. Exit Logic — Tiered Risk Management
7. Position Sizing & Capital Allocation
8. IV Rank Engine
9. Scan Scheduler & Execution Flow
10. Risk Guard System
11. PDT Tracker Implementation
12. Data Requirements & Alpaca Endpoints
13. Config Reference
14. State Machine: Trade Lifecycle
15. Metrics & Performance Tracking
16. Glossary

---

## 1. PDT Rule Compliance & Account Structure

### Does PDT Apply to Options on Alpaca?

**Yes.** PDT applies to ALL securities on Alpaca margin accounts, including options. A day trade is defined as opening and closing the same position on the same calendar day. If an account executes 4+ day trades in a rolling 5-business-day window and those trades represent >6% of total trades in that window, the account gets flagged as a Pattern Day Trader. Flagged accounts must maintain $25,000 minimum equity.

### Strategy: Cash Account + Swing Holding

With a $1,000 account, we cannot afford PDT designation. Our approach:

- **Use a cash account** (not margin) — PDT restrictions technically apply to margin accounts, but Alpaca enforces PDT protection on all accounts under $25k regardless. Using cash eliminates margin call risk entirely.
- **Hold positions overnight minimum** — The core strategy is swing-based (hold 1–5 days minimum). Same-day exits are emergency-only (stop-loss breach, catastrophic news).
- **Track day trades in the daemon** — Maintain a rolling 5-day counter. If day_trade_count >= 3, the system HARD BLOCKS any same-day close unless it's a protective stop-loss on a position losing >50% of premium paid.
- **Emergency day trade budget** — Reserve a maximum of 2 day trades per rolling 5-day window for genuine emergencies. Never use the 3rd.

```
[pdt]
max_day_trades_per_5d   = 2      # hard cap (FINRA allows 3, we stay conservative)
rolling_window_days     = 5
emergency_stop_pct      = 50.0   # only same-day close if losing >50% of premium
block_on_limit          = true   # reject close orders that would breach limit
```

---

## 2. Symbol Universe & Budget Constraints

### Budget Reality Check

With $1,000 total capital, we need options contracts where:
- Premium per contract ≤ $200 (to allow 4–5 simultaneous positions)
- Underlying has sufficient options liquidity (open interest >100, daily volume >50)
- Bid-ask spread is tight (ideally <$0.15 for contracts in our delta range)

### Tiered Symbol List

Symbols are organized by tier based on typical OTM call/put premium affordability and liquidity.

#### Tier 1 — Core (highest liquidity, most consistent premiums, always scan)

```
SPY    — S&P 500 ETF, king of options liquidity, OTM calls/puts $0.50–$3.00
QQQ    — Nasdaq 100 ETF, tech-heavy, excellent liquidity
IWM    — Russell 2000 ETF, small-cap exposure, cheaper premiums than SPY
```

#### Tier 2 — Budget-Friendly High-Liquidity Equities

```
F      — Ford, $10–$14 range, premiums often $0.15–$1.00
SOFI   — SoFi Technologies, volatile fintech, cheap premiums
PLTR   — Palantir, high retail interest = high OI
NIO    — Chinese EV, volatile, cheap premium
RIVN   — Rivian, similar profile to NIO
HOOD   — Robinhood, meta play, liquid options
SNAP   — Snapchat, cheap underlying = cheap options
AAL    — American Airlines, cyclical, good for swing plays
CLF    — Cleveland-Cliffs, steel/materials, follows macro
T      — AT&T, stable dividend stock, very cheap premiums
PFE    — Pfizer, healthcare, cheap and liquid
BAC    — Bank of America, financials exposure
INTC   — Intel, tech turnaround play, cheap premiums
```

#### Tier 3 — Conditional (scan only when IV rank is elevated)

```
AMD    — If premiums drop into range during low-IV periods
AMZN   — Typically too expensive, but far OTM puts can work
AAPL   — Same as AMZN, far OTM only
MARA   — Crypto-correlated, high IV = expensive but can work for puts
COIN   — Coinbase, similar to MARA
```

### Dynamic Symbol Filtering

On each scan, the daemon should:

1. Fetch the options chain for each symbol
2. Filter contracts where `ask_price * 100 <= max_position_usd` (default $200)
3. Filter for minimum open interest and volume thresholds
4. Only proceed with contracts that pass ALL liquidity checks

```
[symbols]
tier1 = ["SPY", "QQQ", "IWM"]
tier2 = ["F", "SOFI", "PLTR", "NIO", "RIVN", "HOOD", "SNAP", "AAL", "CLF", "T", "PFE", "BAC", "INTC"]
tier3 = ["AMD", "AMZN", "AAPL", "MARA", "COIN"]
tier3_iv_rank_min = 40   # only scan tier3 when their IV rank >= 40th percentile

[liquidity]
min_open_interest     = 100
min_daily_volume      = 50
max_bid_ask_spread    = 0.20   # dollars
```

---

## 3. Strategy Overview — Multi-Regime Swing Trading

### Philosophy

This is NOT a directional gambling strategy. This is a **probability-weighted, multi-signal confluence system** that:

1. Identifies the current market regime (trending vs. mean-reverting vs. choppy)
2. Selects the appropriate options strategy for that regime
3. Requires multiple independent signals to agree before entry (confluence gate)
4. Manages risk through position sizing, tiered exits, and time-based decay management
5. Prioritizes capital preservation over aggressive returns

### Target Performance

- **Win rate:** 55–65% (achievable with confluence gating)
- **Average winner:** 20–40% of premium paid
- **Average loser:** 15–25% of premium paid (capped by stop-loss)
- **Risk/reward:** ~1:1.5
- **Monthly target:** 3–8% on capital (compounding)
- **Max drawdown tolerance:** 10% of total capital ($100 on $1,000)

### Strategy Modes

The system operates in one of four modes. **Each mode has its own signal set** (see §5a) — we do not apply trend signals to non-trending markets, which was the core mistake of v2.0.

| Regime | Detection | Sub-strategy | Signal set |
|--------|-----------|--------------|------------|
| **Trending Up** | Price > EMA20 > EMA50, ADX ≥ 22, +DI > -DI | Buy calls on EMA9/20 pullback | Trend-following |
| **Trending Down** | Price < EMA20 < EMA50, ADX ≥ 22, -DI > +DI | Buy puts on EMA9/20 rally | Trend-following |
| **Range-Bound** | ADX < 18, price within BBands, BBand width > 5% | Buy puts at upper band, calls at lower band | Mean-reversion |
| **Choppy** | None of the above | **NO TRADE** unless `allow_choppy = true` AND a strong mean-reversion signal fires (BBand pierce + RSI extreme) | Restricted mean-reversion only, half size |

**Key rule:** A regime must be *positively identified*, not assumed by elimination. If trend tests fail AND range-bound tests fail, the regime is choppy and the default action is no trade. The `allow_choppy` flag (default: false) lets you opt into a *strict* mean-reversion sub-strategy in choppy markets, but with reduced size and stricter exit rules. This is the honest version of what week 1 was doing implicitly.

### Why score variance matters

In week 1, CLF, BAC, SPY, QQQ, IWM all averaged exactly 6.0 across ~150 scans. That's a constant, not a score. It happens because additive scoring with many low-discrimination signals produces a baseline that everything passes. The v2.1 confluence system fixes this with:

- **Veto gates:** A failing IV Rank or a stale-bar condition kills the trade outright. Scores can't compensate.
- **Multiplicative regime gate:** Trend signals only score in trending regimes; range signals only score in range-bound regimes. A symbol in the wrong regime scores zero on the wrong indicator set, not 1 point of partial credit.
- **Quality-weighted points:** Strong signals (RSI divergence at extreme, EMA pullback after a clean trend leg) score more; weak signals (ADX merely > threshold, volume merely "average") score less or are removed entirely.

---

## 4. Technical Indicator Stack

All indicators are computed on the **underlying stock/ETF daily bars** (not on the option itself). Alpaca provides OHLCV bars via the Market Data API; indicators are calculated locally in the daemon.

### Primary Indicators

#### 1. Exponential Moving Averages (EMA 9 / 20 / 50)

- **EMA 9:** Fast signal line, used for entry timing
- **EMA 20:** Intermediate trend, pullback target in trending regimes
- **EMA 50:** Trend filter, regime classifier
- **Calculation:** Standard EMA formula, computed on daily close prices
- **Data needed:** 60 days of daily bars minimum for EMA50 warm-up

```rust
// Regime classification logic
if close > ema20 && ema20 > ema50 { regime = TrendingUp }
if close < ema20 && ema20 < ema50 { regime = TrendingDown }
```

#### 2. RSI (14-period) with Divergence Detection

- **Overbought:** RSI > 70
- **Oversold:** RSI < 30
- **Neutral zone:** 40–60 (no signal)
- **Divergence:** Price makes new high but RSI makes lower high (bearish divergence) or vice versa
- **Usage:** Confirmation signal, NOT primary entry trigger
- **In trending regime:** RSI pullback to 40–50 zone = entry zone for calls (uptrend) or 50–60 for puts (downtrend)

#### 3. MACD (12, 26, 9)

- **Signal line crossover:** MACD line crosses above signal = bullish, below = bearish
- **Histogram:** Positive and growing = strengthening momentum
- **Zero line cross:** Confirms trend direction change
- **Usage:** Momentum confirmation, must agree with EMA regime

#### 4. ADX (14-period) — Average Directional Index

- **ADX > 25:** Strong trend exists (enable trending strategy)
- **ADX < 20:** No trend (enable mean-reversion strategy)
- **ADX 20–25:** Transitional zone (reduce position size by 50% or skip)
- **+DI / -DI:** Confirms trend direction alongside EMAs

#### 5. Bollinger Bands (20-period, 2 std dev)

- **Upper band touch/pierce:** Overbought in range regime → put signal
- **Lower band touch/pierce:** Oversold in range regime → call signal
- **Band width (squeeze):** Narrow bands = low volatility, potential breakout coming
- **Usage:** Primary signal in range-bound regime, confirmation in trending

#### 6. VWAP (Volume-Weighted Average Price)

- **Intraday anchor:** Price above VWAP = bullish bias, below = bearish bias
- **Available directly from Alpaca bars** (vwap field in bar data)
- **Usage:** Entry timing — prefer entries when price is on the "correct" side of VWAP for the trade direction

### Secondary Indicators

#### 7. ATR (14-period) — Average True Range

- Used for **stop-loss placement**, NOT for entry signals
- Stop-loss = entry_price - (1.5 × ATR) for calls, entry_price + (1.5 × ATR) for puts
- Position sizing adjustment: higher ATR = smaller position

#### 8. Volume Confirmation

- Entry volume should be ≥ 1.0× the 20-day average volume
- Breakout entries require ≥ 1.5× average volume
- Low volume signals are downgraded (reduce confidence score)

#### 9. IV Rank (computed from Alpaca snapshot data)

- See Section 8 for full IV Rank engine specification
- Used as a filter: avoid buying options when IV is extremely elevated (IV rank >80)
- Sweet spot for buying: IV rank 20–50

---

## 5. Entry Logic — The Confluence Gate (v2.1 rewrite)

The week 1 data showed that an additive scoring system with many low-discrimination signals produces flat scores that don't actually rank anything. This rewrite changes the model from "add up points until you pass a threshold" to "pass every veto, then earn a quality score, then size accordingly."

### The three-stage gate

Every potential entry passes through three stages **in order**. Failing any stage at any point kills the trade — no compensation, no overrides.

```
   ┌────────────────────┐
   │ STAGE 1: VETOES    │  ← any failure = no trade
   │ (hard pass/fail)   │
   └─────────┬──────────┘
             │ all pass
             ▼
   ┌────────────────────┐
   │ STAGE 2: REGIME    │  ← classifies which signal set to use
   │ (must positively   │
   │  identify regime)  │
   └─────────┬──────────┘
             │ regime ∈ {trend_up, trend_down, range}
             ▼
   ┌────────────────────┐
   │ STAGE 3: SCORING   │  ← regime-specific signals only
   │ (quality score for │
   │  sizing)           │
   └────────────────────┘
```

### Stage 1: Vetoes (hard gates)

These are pre-conditions, not "signals." If any of these fail, the trade is rejected silently — it does not contribute to the score, it ends the evaluation.

| Veto | Condition | Why |
|------|-----------|-----|
| Stale bar | Latest bar timestamp > 90 min old during market hours | Prevents acting on dead data (week 1 had midnight EMA50 exits firing on stale prices) |
| IV Rank too high | IV Rank > 60 | Overpaying for premium; even correct direction loses to IV crush |
| IV Rank too low | IV Rank < 10 | Suspiciously low IV often = data gap, not opportunity |
| Spread too wide | (ask - bid) / mid > 8% on the underlying's most-traded near-money strike | Illiquid = bad fills + bad exits |
| No chain | Options chain returned empty or no contracts in 14–45 DTE band | Free-tier data gap; skip rather than guess |
| Recent gap | Yesterday's close to today's open gapped > 2 ATR | Wait for the dust to settle; gaps create false signals |
| Volume drought | 20-day average volume < 500k shares (underlying) | Thin underlying = thin options |
| Too close to extreme | For calls: today's high is within 0.5 ATR of 20-day high<br>For puts: today's low is within 0.5 ATR of 20-day low | Buying at the local extreme is a high-probability stop-out (this is what killed the NIO trade in 5 minutes) |
| Already exposed | We already hold a position in this underlying or correlated underlying | No doubling down |
| Within minimum cooldown | We closed a position in this underlying within the last 4 hours | No revenge trades |

A symbol that fails any of these doesn't appear in scoring at all. This alone should drop the scan-to-entry rate significantly compared to week 1.

### Stage 2: Regime Classification

The system must **positively identify** one of three actionable regimes. If none match, the regime is **choppy** and no trade fires (unless `allow_choppy = true`, in which case only the restricted mean-reversion path is available).

```
def classify_regime(bars):
    ema9, ema20, ema50 = compute_emas(bars)
    adx, plus_di, minus_di = compute_adx(bars, 14)
    bb_upper, bb_mid, bb_lower = bollinger(bars, 20, 2.0)
    bb_width_pct = (bb_upper - bb_lower) / bb_mid * 100
    close = bars[-1].close

    # Trend tests (must be POSITIVELY true)
    is_trend_up = (
        close > ema20 > ema50 and
        adx >= 22 and
        plus_di > minus_di and
        ema20 > ema20_5_bars_ago  # EMA20 itself rising
    )
    is_trend_down = (
        close < ema20 < ema50 and
        adx >= 22 and
        minus_di > plus_di and
        ema20 < ema20_5_bars_ago
    )

    # Range test (must be POSITIVELY true)
    is_range = (
        adx < 18 and
        bb_lower < close < bb_upper and
        bb_width_pct >= 5.0 and  # band must be wide enough to mean-revert into
        abs(close - ema50) / ema50 < 0.05  # price near the long-term mean
    )

    if is_trend_up:    return Regime.TrendUp
    if is_trend_down:  return Regime.TrendDown
    if is_range:       return Regime.RangeBound
    return Regime.Choppy
```

Note that the trend and range tests **cannot both be true** (ADX ≥ 22 and ADX < 18 are disjoint). This is intentional — regime is unambiguous.

### Stage 3: Quality Scoring (regime-specific)

Each regime has its own signal set. Trend signals are **not** scored in range regimes, and vice versa. This is the fix for week 1's flat scores.

#### Trend-Up (long calls) — max score 12

| Signal | Points | Condition |
|--------|--------|-----------|
| Pullback to EMA9 | 3 | `low <= ema9 <= high` on current bar AND prior 2 bars closed above ema9 |
| Pullback to EMA20 | 2 | `low <= ema20 <= high` on current bar (deeper pullback, slightly less ideal) |
| RSI in buy zone | 2 | RSI(14) between 40 and 55 (pulled back but not broken) |
| MACD histogram turning up | 2 | Histogram negative or near zero, but rising for 2 consecutive bars |
| Higher low confirmation | 2 | Today's low > the low of 5 bars ago (structural higher low) |
| Volume contraction on pullback | 1 | Today's volume < 0.9 × 20-day average (clean pullback, not distribution) |
| ADX rising | 1 | ADX(14) today > ADX 3 bars ago (trend strengthening) |
| RSI bullish divergence | +2 bonus | Lower price low + higher RSI low over last 10 bars |

**Minimum score: 7/12.** Below 7, the setup isn't clean enough.

Note: pullback to EMA9 and pullback to EMA20 are mutually exclusive — you score at most one of them. If price is between them, score the EMA9 row (closer = better).

#### Trend-Down (long puts) — max score 12

Mirror image of Trend-Up. Pullbacks become rallies, "higher low" becomes "lower high," RSI buy zone becomes 45–60.

#### Range-Bound (mean-reversion) — max score 10

| Signal | Points | Condition |
|--------|--------|-----------|
| Band touch | 3 | For calls: today's low <= lower BBand. For puts: today's high >= upper BBand |
| RSI extreme | 3 | For calls: RSI(14) ≤ 30. For puts: RSI(14) ≥ 70 |
| Reversal candle | 2 | For calls: today's close > today's open AND lower wick > 0.5 × body. For puts: mirror |
| Distance from mean | 1 | For calls: close is at least 1.5 ATR below ema20. For puts: at least 1.5 ATR above |
| Volume spike on touch | 1 | Today's volume > 1.2 × 20-day average (capitulation/exhaustion) |

**Minimum score: 6/10.** This is a higher bar relative to max than the trend setups, because mean-reversion in a non-trending market needs cleaner setups to overcome the lack of directional tailwind.

#### Choppy (restricted, only if `allow_choppy = true`)

Same signal set as Range-Bound, but **minimum score 8/10** (almost perfect setup) and **size factor 0.5** (half the normal position). This is the "we know it's risky but we want to keep the bot working" sub-strategy.

### Score → action mapping

Score determines two things: whether to enter, and how big.

| Regime | Min score | Sizing tiers (score → size factor) |
|--------|-----------|-----------------------------------|
| TrendUp / TrendDown | 6/12 | 6-8: 0.5×  •  9-10: 0.75×  •  11-12: 1.0× |
| RangeBound | 6/10 | 6: 0.5×  •  7-8: 0.75×  •  9-10: 1.0× |
| Choppy (if enabled) | 8/10 | 8-10: 0.5× (always half) |

The size factor is multiplied against `max_position_usd` to get the dollar budget for that trade.

### Entry procedure (revised)

```
1. Fetch latest daily bars (with staleness check)
2. STAGE 1: Run all vetoes. Any failure → reject silently.
3. STAGE 2: Classify regime.
4. If regime is Choppy and allow_choppy is false → reject.
5. STAGE 3: Score using the regime-appropriate signal set.
6. If score < min for that regime → reject.
7. Determine direction (long calls or long puts) from regime.
8. Look up size factor from score tier.
9. Compute target position dollars = max_position_usd × size_factor.
10. Fetch options chain.
11. Filter contracts:
    a. Right (call/put) per direction
    b. Delta 0.30–0.45
    c. DTE 14–45
    d. Premium × 100 ≤ target position dollars
    e. Open interest ≥ 100, volume ≥ 50, spread ≤ $0.20
12. Rank: delta closest to 0.35 → tightest spread → highest OI.
13. Submit limit order at mid-price.
14. Tag the position with regime, score, and entry timestamp for exit logic.
```

### What this fixes (vs. week 1)

| Week 1 problem | v2.1 fix |
|----------------|----------|
| CLF, BAC, SPY scoring exactly 6.0 every scan | Regime-specific scoring + vetoes; flat baselines no longer pass |
| 69% of scans were choppy and got traded anyway | Choppy is now an explicit decision (`allow_choppy`), not the default |
| NIO stopped out 5 min after entry | "Too close to extreme" veto + minimum hold time (§6) |
| EMA50 trend signal applied to non-trending markets | Trend signals only score in trend regimes |
| 1,038 entries / 3,907 scans (27%) | Expected v2.1 entry rate: 3-8% of scans |

### Contract Selection Preferences

```toml
[strategy.entry]
# Vetoes
stale_bar_max_minutes      = 90
iv_rank_veto_high          = 60
iv_rank_veto_low           = 10
underlying_spread_pct_max  = 8.0
recent_gap_atr             = 2.0
min_avg_volume_shares      = 500_000
extreme_proximity_atr      = 0.5
cooldown_after_close_hours = 4

# Regime classification
adx_trend_min              = 22
adx_range_max              = 18
bb_width_min_pct           = 5.0
ema_slope_lookback_bars    = 5

# Scoring thresholds
trend_min_score            = 7
range_min_score            = 6
choppy_min_score           = 8
allow_choppy               = false  # default off; flip to true at your own risk

# Contract filters (unchanged from v2.0)
preferred_delta            = 0.35
delta_min                  = 0.30
delta_max                  = 0.45
dte_min                    = 14
dte_max                    = 45
order_type                 = "limit"
limit_price_method         = "mid"
```

---

## 6. Exit Logic — Tiered Risk Management

### Exit is where money is made or saved. This section is critical.

The system uses a **three-tier exit framework**, plus two new gates added in v2.1:

- **Market hours gate:** Exit signals can only fire during regular market hours (9:30–16:00 ET). Signals computed outside that window are queued for the next open. Week 1 had three exits fire at 00:00 UTC on stale prices — that bug is closed by this gate.
- **Minimum hold gate:** For non-emergency exits, a position must be held for at least `min_hold_minutes` (default: 60 minutes of market time, ~1 trading hour). This prevents the "stopped out in 5 minutes" pattern that hit NIO in week 1. Stop-loss can still fire inside the hold window only if loss exceeds `emergency_stop_pct` (-50%).

### Tier 1: Profit Target (Happy Path)

| Condition | Action |
|-----------|--------|
| Unrealized P&L >= +30% of premium paid | Close 50% of position (if >1 contract) |
| Unrealized P&L >= +50% of premium paid | Close remaining position |
| If only 1 contract: P&L >= +40% | Close entire position |

**Rationale:** Taking profits at 30–50% of premium is the quant standard for long options. Holding for "home runs" dramatically reduces win rate with a small account.

### Tier 2: Stop-Loss (Capital Preservation)

| Condition | Action | Honors min hold? |
|-----------|--------|------------------|
| Unrealized P&L <= -30% of premium paid | Close entire position | Yes (min hold applies) |
| Unrealized P&L <= -50% of premium paid (emergency) | Close immediately | **No** (overrides min hold) |
| Underlying breaks EMA50 against position, AND position held > min_hold_minutes | Close position | Yes |
| Confluence score on re-evaluation drops below 4 | Close position | Yes |

**Stop-loss is NON-NEGOTIABLE.** The daemon must enforce this mechanically. No manual override. No "let it ride." A 30% loss on a $200 position is $60. A 100% loss is $200. The difference compounds quickly on a $1,000 account.

**v2.1 change:** The min-hold rule means a stop-loss inside the first 60 minutes is *only* allowed at the emergency threshold. This is a deliberate trade-off: we accept slightly larger occasional losses in exchange for not getting shaken out by 5-minute noise.

### Tier 3: Time-Based Decay Management

| Condition | Action |
|-----------|--------|
| DTE remaining <= 10 | Close position regardless of P&L |
| DTE remaining <= 7 AND P&L < +10% | Close immediately (theta is eating alive) |
| Held for 5+ trading days with < 5% move | Close (dead money, redeploy capital) |

**Rationale:** Theta decay accelerates dramatically inside 2 weeks. With our 14–45 DTE entry window, we should never be holding at expiration. If the thesis hasn't played out by day 10, it's not going to.

### Exit Priority (highest to lowest)

```
1. Emergency stop-loss (-50%)        → IMMEDIATE close (overrides min hold + market hours where legal)
2. Stop-loss breach (-30%)           → close (subject to min hold + market hours)
3. DTE <= 7 with < +10% P&L          → close
4. Profit target hit (+50%)          → close
5. EMA50 break (thesis dead)         → close (subject to min hold)
6. Time decay (DTE <= 10)            → close
7. Dead money (5 days, < 5%)         → close
8. Confluence score collapse         → close (subject to min hold)
```

### Exit gates summary

```
def can_exit(position, exit_reason, now):
    # Emergency always allowed during market hours
    if exit_reason == "emergency_stop":
        return market_open(now)

    # All other exits require market hours
    if not market_open(now):
        queue_for_next_open(position, exit_reason)
        return False

    # Profit targets and time-based exits ignore min hold
    if exit_reason in ("profit_target", "time_decay", "dead_money"):
        return True

    # Stop-loss and thesis exits honor min hold
    held_minutes = (now - position.opened_at).market_minutes()
    if held_minutes < min_hold_minutes:
        log(f"{exit_reason} suppressed: only held {held_minutes} min")
        return False

    return True
```

### PDT-Aware Exit Logic

Before submitting any exit order:

```
1. Check: Was this position opened today?
2. If YES:
   a. Check day_trade_count in rolling 5-day window
   b. If day_trade_count >= max_day_trades_per_5d (2):
      - BLOCK the exit UNLESS stop-loss is at emergency threshold (-50%)
      - Log warning: "PDT limit reached, holding overnight"
      - Set position.force_exit_next_open = true
   c. If day_trade_count < max_day_trades_per_5d:
      - Allow exit but increment counter
      - Log: "Day trade #{n} used — emergency exit"
3. If NO (opened on a prior day):
   - Allow exit normally, does NOT count as day trade
```

```
[exit]
profit_target_partial_pct = 30.0    # take 50% off at +30%
profit_target_full_pct    = 50.0    # close remainder at +50%
profit_target_single_pct  = 40.0    # if qty=1, close at +40%
stop_loss_pct             = 30.0    # max loss per position
emergency_stop_pct        = 50.0    # allows same-day exit even near PDT limit
time_exit_dte             = 10      # close if DTE drops to this
theta_exit_dte            = 7       # close if DTE here AND P&L < threshold
theta_exit_min_pnl_pct    = 10.0    # minimum P&L to hold past theta_exit_dte
dead_money_days           = 5       # close if held this long with < threshold move
dead_money_min_pct        = 5.0     # minimum move to justify continued hold
```

---

## 7. Position Sizing & Capital Allocation

### Kelly-Inspired Position Sizing (Conservative)

With $1,000 and an expectation of 55–65% win rate:

```
max_risk_per_trade     = 2.5%   # $25 max risk on $1,000
max_position_usd       = 200    # max premium per contract (single leg)
max_portfolio_risk_pct = 15.0   # max total capital at risk across all open positions
max_open_positions     = 4      # diversification floor
min_cash_reserve_pct   = 30.0   # always keep 30% cash ($300 on $1,000)
```

### Dynamic Sizing Based on Confidence

Sizing is regime-specific in v2.1 because the score ranges differ between regimes (trend max 12, range max 10). The full table lives in §5 and the config in §13. The short version:

| Regime | Score range | Size factor → dollars (on $200 base) |
|--------|-------------|---------------------------------------|
| Trend (up/down) | 7–8 | 0.50× → $100 |
| Trend (up/down) | 9–10 | 0.75× → $150 |
| Trend (up/down) | 11–12 | 1.00× → $200 |
| Range-Bound | 6 | 0.50× → $100 |
| Range-Bound | 7–8 | 0.75× → $150 |
| Range-Bound | 9–10 | 1.00× → $200 |
| Choppy (if enabled) | 8–10 | 0.50× → $100 (always half) |

### Capital Allocation Rules

```
available_capital = account_equity - (account_equity * min_cash_reserve_pct / 100)
total_at_risk     = sum(open_position_premiums)

if total_at_risk >= available_capital * max_portfolio_risk_pct / 100:
    BLOCK new entries → log "Portfolio risk limit reached"

if open_positions >= max_open_positions:
    BLOCK new entries → log "Max positions reached"
```

### Sector Diversification

Never hold more than 2 positions in the same sector:

```
sector_map:
  tech:        [QQQ, PLTR, INTC, SNAP, HOOD, AAPL, AMD, AMZN]
  financials:  [SOFI, BAC, COIN]
  automotive:  [F, NIO, RIVN]
  healthcare:  [PFE]
  telecom:     [T]
  airlines:    [AAL]
  materials:   [CLF]
  broad_mkt:   [SPY, IWM]
  crypto_adj:  [MARA, COIN]
```

---

## 8. IV Rank Engine

### Why IV Rank Matters

Implied Volatility (IV) tells you how expensive options premiums are right now. IV Rank tells you where current IV sits relative to its own history. Buying options when IV Rank is high means you're overpaying; IV contraction will work against you even if direction is correct.

### Computing IV Rank

Alpaca provides current IV via the options chain snapshot endpoint (`impliedVolatility` field). To compute IV Rank, you need historical IV data.

**Method: Rolling 52-Week IV Rank**

```
iv_rank = (current_iv - iv_52w_low) / (iv_52w_high - iv_52w_low) * 100
```

**Bootstrap approach (since Alpaca historical options data starts Feb 2024):**

1. On daemon startup, fetch 252 trading days of daily bars for each underlying
2. Compute historical volatility (HV) using 20-day rolling standard deviation of log returns, annualized:
   ```
   daily_returns = ln(close[i] / close[i-1])
   hv_20 = std(daily_returns[-20:]) * sqrt(252)
   ```
3. Use HV as a proxy for IV baseline until you've collected enough live IV snapshots
4. Store daily IV snapshots in SQLite — after 30+ days of collection, switch to actual IV data for rank calculation
5. After 252 days of IV collection, you have a true 52-week IV Rank

**Simplified IV Rank (for early operation before full history):**

Use HV Percentile as a proxy:
```
hv_rank = (current_hv_20 - hv_52w_low) / (hv_52w_high - hv_52w_low) * 100
```

### IV Rank Trading Rules

| IV Rank | Interpretation | Action |
|---------|---------------|--------|
| 0–20 | IV extremely low | Good for buying. Cheap premiums. Enter if signals align. |
| 20–50 | IV moderate | **Sweet spot for buying.** Preferred entry zone. |
| 50–70 | IV elevated | Proceed with caution. Reduce position size by 25%. |
| 70–100 | IV extremely high | **DO NOT BUY.** Premiums are inflated. Wait for IV crush. |

```
[iv_engine]
iv_rank_buy_max        = 60     # don't buy options above this IV rank
iv_rank_sweet_min      = 20     # preferred lower bound
iv_rank_sweet_max      = 50     # preferred upper bound
iv_rank_caution_min    = 50     # reduce size above this
iv_rank_caution_factor = 0.75   # multiply position size by this in caution zone
hv_lookback_days       = 20     # period for historical volatility calculation
iv_history_table       = "iv_snapshots"  # SQLite table for IV history
```

---

## 9. Scan Scheduler & Execution Flow

### Scan Timing

Options markets are open 9:30 AM – 4:00 PM ET. The strategy avoids the first and last 15 minutes (high spread volatility).

```
[scheduler]
scan_start_time        = "09:45"    # ET — skip opening chaos
scan_end_time          = "15:45"    # ET — skip closing chaos
primary_scan_interval  = 300       # seconds (5 min) — indicator check
chain_scan_interval    = 900       # seconds (15 min) — full options chain fetch
exit_check_interval    = 60        # seconds (1 min) — check exits on open positions
market_data_cooldown   = 2         # seconds between API calls (respect rate limits)
```

### Execution Flow Per Scan Cycle

```
┌─────────────────────────────────────────────────────────────┐
│  SCAN CYCLE (every primary_scan_interval)                   │
│                                                             │
│  1. Fetch daily bars for all active symbols (60-day window) │
│  2. Compute indicators: EMA9/20/50, RSI14, MACD, ADX,      │
│     BBands, ATR14, Volume ratio                             │
│  3. Determine regime per symbol                             │
│  4. Score confluence per symbol + direction                 │
│                                                             │
│  IF any symbol scores >= min_confluence_score:              │
│  5. Fetch options chain (snapshot) for qualifying symbols   │
│  6. Compute IV Rank                                        │
│  7. Filter contracts (delta, DTE, premium, liquidity)       │
│  8. Check position limits, capital, sector diversification  │
│  9. Generate entry signal → submit limit order at mid       │
│                                                             │
│  ALWAYS (every exit_check_interval):                        │
│  10. Fetch current positions from Alpaca                    │
│  11. Fetch latest quotes for held contracts                 │
│  12. Evaluate exit conditions (profit/stop/time/dead money) │
│  13. Submit exit orders as needed (PDT-aware)               │
│  14. Update PDT counter                                     │
│  15. Log all signals, decisions, and state to SQLite + TUI  │
└─────────────────────────────────────────────────────────────┘
```

---

## 10. Risk Guard System

The risk guard runs **before every order submission** (entry or exit) and can VETO any order.

### Guard Checks (in order)

```
1. ACCOUNT EQUITY CHECK
   - Fetch account from Alpaca
   - If equity < $100 → HALT ALL TRADING, alert via TUI
   - If equity dropped >10% from session start → HALT for the day

2. DAILY DRAWDOWN CHECK
   - Calculate: (starting_equity - current_equity) / starting_equity * 100
   - If drawdown >= daily_drawdown_pct (3%) → HALT new entries for the day
   - Existing exit orders still allowed

3. PDT CHECK (for exit orders only)
   - If this exit would create a day trade:
     - If day_trade_count >= max_day_trades_per_5d → BLOCK
     - Unless loss exceeds emergency_stop_pct → ALLOW with warning

4. POSITION LIMIT CHECK (for entry orders only)
   - If open_positions >= max_open_positions → BLOCK
   - If total_risk >= max_portfolio_risk → BLOCK
   - If sector_count[symbol_sector] >= 2 → BLOCK

5. CAPITAL CHECK (for entry orders only)
   - If order_cost > available_buying_power → BLOCK
   - If order_cost would reduce cash below min_cash_reserve → BLOCK

6. SANITY CHECK
   - If order price is >20% away from mid-price → BLOCK (likely bad data)
   - If contract DTE < dte_min at time of entry → BLOCK
   - If contract has no bid (illiquid) → BLOCK
```

```
[risk]
max_position_usd       = 200
daily_drawdown_pct     = 3.0
max_open_positions     = 4
min_cash_reserve_pct   = 30.0
max_portfolio_risk_pct = 15.0
halt_equity_floor      = 100    # absolute dollar floor
price_sanity_pct       = 20.0   # reject orders >20% from mid
max_sector_positions   = 2
```

---

## 11. PDT Tracker Implementation

### Data Structure

```rust
struct PdtTracker {
    // Ring buffer of day trades with timestamps
    day_trades: Vec<DayTrade>,
}

struct DayTrade {
    symbol: String,          // contract symbol
    open_time: DateTime,     // when position was opened
    close_time: DateTime,    // when position was closed
    // A day trade = open_time.date() == close_time.date()
}

impl PdtTracker {
    fn count_in_window(&self) -> usize {
        let cutoff = now() - 5_business_days;
        self.day_trades.iter()
            .filter(|dt| dt.close_time >= cutoff)
            .count()
    }

    fn can_day_trade(&self) -> bool {
        self.count_in_window() < MAX_DAY_TRADES_PER_5D
    }

    fn can_emergency_exit(&self) -> bool {
        // Always allow if loss exceeds emergency threshold
        // Even if at PDT limit
        true
    }

    fn would_be_day_trade(&self, position: &Position) -> bool {
        position.opened_at.date() == today()
    }
}
```

### SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS day_trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contract_symbol TEXT NOT NULL,
    underlying TEXT NOT NULL,
    open_time TEXT NOT NULL,       -- ISO 8601
    close_time TEXT NOT NULL,      -- ISO 8601
    open_price REAL NOT NULL,
    close_price REAL NOT NULL,
    pnl REAL NOT NULL,
    was_emergency BOOLEAN DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS iv_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol TEXT NOT NULL,           -- underlying symbol
    timestamp TEXT NOT NULL,        -- ISO 8601
    implied_volatility REAL,
    historical_volatility_20 REAL,
    iv_rank REAL,
    UNIQUE(symbol, timestamp)
);

CREATE TABLE IF NOT EXISTS trade_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contract_symbol TEXT NOT NULL,
    underlying TEXT NOT NULL,
    direction TEXT NOT NULL,        -- "call" or "put"
    action TEXT NOT NULL,           -- "open" or "close"
    timestamp TEXT NOT NULL,
    price REAL NOT NULL,
    quantity INTEGER NOT NULL,
    confluence_score INTEGER,
    regime TEXT,                    -- "trending_up", "trending_down", "range", "choppy"
    iv_rank REAL,
    delta REAL,
    dte INTEGER,
    exit_reason TEXT,               -- null for opens
    pnl REAL                        -- null for opens
);
```

---

## 12. Data Requirements & Alpaca Endpoints

### Endpoints Used

| Purpose | Endpoint | Frequency |
|---------|----------|-----------|
| Daily bars (underlying) | `GET /v2/stocks/{symbol}/bars` | Every scan cycle (cached intraday) |
| Latest bar | `GET /v2/stocks/{symbol}/bars/latest` | Every scan cycle |
| Options chain + Greeks | `GET /v1beta1/options/snapshots/{underlyingSymbol}` (chain endpoint) | When confluence score met |
| Options snapshot | `GET /v1beta1/options/snapshots` | For position monitoring |
| Account info | `GET /v2/account` | Every risk guard check |
| Positions | `GET /v2/positions` | Every exit check cycle |
| Submit order | `POST /v2/orders` | On entry/exit signals |
| Cancel order | `DELETE /v2/orders/{order_id}` | On stale unfilled orders |
| Order status | `GET /v2/orders` | Order tracking |
| Market clock | `GET /v2/clock` | Scheduler (is market open?) |
| Market calendar | `GET /v2/calendar` | Pre-compute trading days for PDT window |

### Data Caching Strategy

- **Daily bars:** Fetch full 60-day window once at market open. During the day, append only the latest bar. Full refresh on each new trading day.
- **Options chain:** No caching — always fetch fresh (Greeks change constantly).
- **IV snapshots:** Store one snapshot per symbol per day in SQLite for IV Rank history.
- **Account/Positions:** Fetch fresh on every check (these change with fills).

### API Rate Limiting

Alpaca allows up to 200 API calls/min on free plan and 1,000/min on Algo Trader Plus. The daemon should:

- Implement a token bucket rate limiter
- Space requests by `market_data_cooldown` seconds minimum
- Queue non-urgent requests (IV snapshots) behind urgent ones (exit checks)
- Log and alert if approaching rate limit

---

## 13. Config Reference

Complete `config.toml` for the iron-conduit strategy:

```toml
[alpaca.paper]
key_id     = "PAPER_KEY_ID"
secret_key = "PAPER_SECRET_KEY"
base_url   = "https://paper-api.alpaca.markets"
data_url   = "https://data.alpaca.markets"

[alpaca.live]
enabled    = false    # HARD BLOCK in V1
key_id     = ""
secret_key = ""

# ──────────────────────────────────────
# Symbol Universe
# ──────────────────────────────────────
[symbols]
tier1 = ["SPY", "QQQ", "IWM"]
tier2 = ["F", "SOFI", "PLTR", "NIO", "RIVN", "HOOD", "SNAP", "AAL", "CLF", "T", "PFE", "BAC", "INTC"]
tier3 = ["AMD", "AMZN", "AAPL", "MARA", "COIN"]
tier3_iv_rank_min = 40

[liquidity]
min_open_interest     = 100
min_daily_volume      = 50
max_bid_ask_spread    = 0.20

# ──────────────────────────────────────
# Strategy: Iron Conduit
# ──────────────────────────────────────
[strategy]
name                   = "iron-conduit"
scan_interval_secs     = 300
chain_scan_interval    = 900
exit_check_interval    = 60
scan_start_time        = "09:45"
scan_end_time          = "15:45"
market_data_cooldown   = 2

[strategy.entry]
# ── Vetoes (Stage 1) ──
stale_bar_max_minutes      = 90
iv_rank_veto_high          = 60
iv_rank_veto_low           = 10
underlying_spread_pct_max  = 8.0
recent_gap_atr             = 2.0
min_avg_volume_shares      = 500_000
extreme_proximity_atr      = 0.5
cooldown_after_close_hours = 4

# ── Scoring thresholds (Stage 3) ──
trend_min_score            = 7   # max 12
range_min_score            = 6   # max 10
choppy_min_score           = 8   # max 10
allow_choppy               = false

# ── Contract filters ──
preferred_delta            = 0.35
delta_min                  = 0.30
delta_max                  = 0.45
dte_min                    = 14
dte_max                    = 45
order_type                 = "limit"
limit_price_method         = "mid"

[strategy.exit]
# ── New v2.1 gates ──
min_hold_minutes           = 60     # min market-time hold before non-emergency exits arm
market_hours_only          = true   # exit signals queue if outside RTH

# ── Profit targets ──
profit_target_partial_pct  = 30.0
profit_target_full_pct     = 50.0
profit_target_single_pct   = 40.0

# ── Stops ──
stop_loss_pct              = 30.0
emergency_stop_pct         = 50.0   # overrides min_hold_minutes

# ── Time / decay ──
time_exit_dte              = 10
theta_exit_dte             = 7
theta_exit_min_pnl_pct     = 10.0
dead_money_days            = 5
dead_money_min_pct         = 5.0

# ──────────────────────────────────────
# Regime Detection
# ──────────────────────────────────────
[strategy.regime]
ema_fast               = 9
ema_mid                = 20
ema_slow               = 50
ema_slope_lookback     = 5      # bars to check for EMA20 slope
adx_period             = 14
adx_trend_min          = 22     # was 25 in v2.0; tightened from week 1's 22 stays
adx_range_max          = 18     # was 20 in v2.0; widens range zone
bb_width_min_pct       = 5.0    # min BBand width to qualify as range
range_mean_proximity   = 0.05   # close must be within 5% of EMA50 for range
rsi_period             = 14
rsi_overbought         = 70
rsi_oversold           = 30
macd_fast              = 12
macd_slow              = 26
macd_signal            = 9
bbands_period          = 20
bbands_std_dev         = 2.0
atr_period             = 14
volume_ma_period       = 20

# ──────────────────────────────────────
# IV Rank Engine
# ──────────────────────────────────────
[iv_engine]
iv_rank_buy_max        = 60
iv_rank_sweet_min      = 20
iv_rank_sweet_max      = 50
iv_rank_caution_min    = 50
iv_rank_caution_factor = 0.75
hv_lookback_days       = 20

# ──────────────────────────────────────
# Position Sizing
# ──────────────────────────────────────
[sizing]
max_risk_per_trade_pct = 2.5
max_position_usd       = 200
max_portfolio_risk_pct = 15.0
max_open_positions     = 4
min_cash_reserve_pct   = 30.0
max_sector_positions   = 2

# Score-based sizing tiers (regime-aware, see §5)
# Trend regime: max score 12, min 7
[[sizing.tiers.trend]]
score_min = 7
score_max = 8
size_factor = 0.50

[[sizing.tiers.trend]]
score_min = 9
score_max = 10
size_factor = 0.75

[[sizing.tiers.trend]]
score_min = 11
score_max = 12
size_factor = 1.00

# Range regime: max score 10, min 6
[[sizing.tiers.range]]
score_min = 6
score_max = 6
size_factor = 0.50

[[sizing.tiers.range]]
score_min = 7
score_max = 8
size_factor = 0.75

[[sizing.tiers.range]]
score_min = 9
score_max = 10
size_factor = 1.00

# Choppy regime: only fires if allow_choppy=true, always half size
[[sizing.tiers.choppy]]
score_min = 8
score_max = 10
size_factor = 0.50

# ──────────────────────────────────────
# Risk Guard
# ──────────────────────────────────────
[risk]
daily_drawdown_pct     = 3.0
halt_equity_floor      = 100
price_sanity_pct       = 20.0

# ──────────────────────────────────────
# PDT Protection
# ──────────────────────────────────────
[pdt]
max_day_trades_per_5d  = 2
rolling_window_days    = 5
emergency_stop_pct     = 50.0
block_on_limit         = true
```

---

## 14. State Machine: Trade Lifecycle

```
                    ┌──────────────┐
                    │   SCANNING   │◄──────────────────────────────┐
                    └──────┬───────┘                               │
                           │ confluence >= 8                       │
                           ▼                                       │
                    ┌──────────────┐                               │
                    │  QUALIFYING  │                               │
                    │  (chain scan)│                               │
                    └──────┬───────┘                               │
                           │ contract found                        │
                           ▼                                       │
                    ┌──────────────┐                               │
                    │  RISK CHECK  │──── BLOCKED ─────────────────►│
                    └──────┬───────┘                               │
                           │ PASSED                                │
                           ▼                                       │
                    ┌──────────────┐                               │
                    │ORDER PENDING │                               │
                    │ (limit @ mid)│                               │
                    └──────┬───────┘                               │
                           │                                       │
              ┌────────────┼────────────┐                          │
              ▼            ▼            ▼                          │
        ┌──────────┐ ┌──────────┐ ┌──────────┐                    │
        │  FILLED  │ │ PARTIAL  │ │ EXPIRED/ │────────────────────►│
        └────┬─────┘ └────┬─────┘ │CANCELLED │                    │
             │            │       └──────────┘                     │
             ▼            ▼                                        │
        ┌──────────────────┐                                       │
        │     HOLDING      │◄─── monitor every exit_check_interval │
        │  (position open) │                                       │
        └────────┬─────────┘                                       │
                 │                                                  │
    ┌────────────┼──────────────────────┐                          │
    ▼            ▼            ▼         ▼                          │
┌────────┐ ┌─────────┐ ┌─────────┐ ┌────────┐                    │
│ PROFIT │ │  STOP   │ │  TIME   │ │  DEAD  │                    │
│ TARGET │ │  LOSS   │ │  DECAY  │ │ MONEY  │                    │
└───┬────┘ └───┬─────┘ └───┬─────┘ └───┬────┘                    │
    │          │           │           │                           │
    ▼          ▼           ▼           ▼                           │
┌──────────────────────────────────────────┐                       │
│          PDT CHECK + EXIT ORDER          │                       │
│  (may hold overnight if PDT blocked)     │                       │
└────────────────────┬─────────────────────┘                       │
                     │                                              │
                     ▼                                              │
              ┌──────────────┐                                      │
              │    CLOSED    │──────────────────────────────────────┘
              │  (log trade) │
              └──────────────┘
```

---

## 15. Metrics & Performance Tracking

### Track These Metrics in SQLite

```sql
-- Computed after each closed trade
win_rate              -- wins / total_trades * 100
avg_win_pct           -- average % gain on winning trades
avg_loss_pct          -- average % loss on losing trades
profit_factor         -- gross_wins / gross_losses
max_drawdown_pct      -- worst peak-to-trough equity decline
sharpe_ratio          -- (avg_return - risk_free) / std_dev(returns)
avg_holding_days      -- mean days held before exit
regime_accuracy       -- % of regime classifications that led to winners
confluence_calibration -- win rate bucketed by confluence score
pdt_trades_used       -- day trades used in current 5-day window
capital_utilization    -- avg % of capital deployed
```

### Weekly Review Queries

```sql
-- Win rate by regime
SELECT regime,
       COUNT(*) as trades,
       SUM(CASE WHEN pnl > 0 THEN 1 ELSE 0 END) * 100.0 / COUNT(*) as win_rate
FROM trade_log
WHERE action = 'close' AND timestamp > date('now', '-7 days')
GROUP BY regime;

-- Confluence score calibration
SELECT confluence_score,
       COUNT(*) as trades,
       AVG(pnl) as avg_pnl,
       SUM(CASE WHEN pnl > 0 THEN 1 ELSE 0 END) * 100.0 / COUNT(*) as win_rate
FROM trade_log
WHERE action = 'close'
GROUP BY confluence_score
ORDER BY confluence_score;

-- Capital efficiency
SELECT date(timestamp) as day,
       SUM(CASE WHEN action = 'open' THEN price * quantity * 100 ELSE 0 END) as deployed,
       SUM(CASE WHEN action = 'close' THEN pnl ELSE 0 END) as daily_pnl
FROM trade_log
GROUP BY date(timestamp)
ORDER BY day DESC;
```

---

## 16. Glossary

| Term | Definition |
|------|-----------|
| **Confluence** | Multiple independent signals agreeing on the same trade direction |
| **DTE** | Days to Expiration — calendar days until the option expires |
| **Delta** | Rate of change of option price per $1 move in underlying. Also approximates probability of expiring ITM |
| **IV** | Implied Volatility — market's expectation of future price movement, baked into option premium |
| **IV Rank** | Where current IV sits relative to its 52-week range (0–100 scale) |
| **Theta** | Daily time decay — how much premium the option loses per day |
| **Vega** | Sensitivity of option price to changes in IV |
| **ATR** | Average True Range — volatility measure used for stop-loss sizing |
| **ADX** | Average Directional Index — measures trend strength (not direction) |
| **EMA** | Exponential Moving Average — weighted toward recent prices |
| **VWAP** | Volume-Weighted Average Price — institutional benchmark price |
| **BBands** | Bollinger Bands — volatility envelope around a moving average |
| **MACD** | Moving Average Convergence Divergence — momentum oscillator |
| **RSI** | Relative Strength Index — momentum oscillator (0–100 scale) |
| **OI** | Open Interest — total outstanding contracts for a specific option |
| **PDT** | Pattern Day Trader — FINRA designation for accounts with 4+ day trades in 5 business days |
| **Regime** | Current market behavior mode (trending, range-bound, or choppy) |
| **Mid-price** | (bid + ask) / 2 — fair value estimate for limit orders |
| **Premium** | The price paid for an options contract (price × 100 shares per contract) |

---

## Implementation Priority for Claude Code

### Phase 1: Foundation (implement first)
1. Config parsing (all sections above)
2. Alpaca client wrapper (bars, chain, orders, positions, account)
3. SQLite schema + migrations
4. PDT tracker
5. Risk guard system

### Phase 2: Strategy Engine
6. Indicator computation (EMA, RSI, MACD, ADX, BBands, ATR, Volume)
7. Regime detection
8. Confluence scoring
9. IV Rank engine (start with HV proxy)

### Phase 3: Execution
10. Contract selection + filtering
11. Entry order submission
12. Exit monitoring loop
13. Tiered exit execution (profit/stop/time/dead money)
14. PDT-aware exit gating

### Phase 4: Observability
15. Trade logging to SQLite
16. TUI signal display (integrate with ferrum-tui IPC)
17. Performance metrics computation
18. Weekly review query support

---

*This strategy document is the authoritative reference for Ferrum's trading logic. All implementation decisions should defer to the specifications here. When in doubt, prioritize capital preservation over profit maximization.*

*Disclaimer: This is for educational and paper trading purposes. Options trading involves significant risk of loss. Past performance does not guarantee future results. Always validate strategies thoroughly in paper trading before risking real capital.*
