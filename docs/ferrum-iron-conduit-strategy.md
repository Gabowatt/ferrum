# Ferrum Trading Strategy: Iron Conduit

> **Version:** 2.0  
> **Codename:** `iron-conduit`  
> **Account Size:** $1,000  
> **Account Type:** Cash (avoids margin requirements, simplifies PDT handling)  
> **Platform:** Alpaca Trading API (paper → live via Algo Trader Plus)  
> **Asset Class:** US equity & ETF options (American-style)

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

The system operates in one of three modes based on regime detection:

| Regime | Detection | Strategy | Direction |
|--------|-----------|----------|-----------|
| **Trending Up** | Price > EMA20 > EMA50, ADX > 22 | Buy calls on pullbacks to EMA20 | Long calls |
| **Trending Down** | Price < EMA20 < EMA50, ADX > 22 | Buy puts on rallies to EMA20 | Long puts |
| **Range-Bound** | ADX < 17, price oscillating in BBands | Buy at band extremes, direction of reversion | Calls at lower band, puts at upper band |
| **Choppy** | ADX 17–22, conflicting signals | Treat as Range-Bound — iron condors thrive in oscillating markets | Calls near lower band, puts near upper band |

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

- **ADX > 22:** Strong trend exists (enable trending strategy; threshold lowered from 25)
- **ADX < 17:** No trend (enable mean-reversion strategy; threshold lowered from 20)
- **ADX 17–22:** Choppy zone — treated as Range-Bound for iron condor purposes
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

## 5. Entry Logic — The Confluence Gate

### Signal Scoring System

Each indicator generates a score. Entry requires a **minimum confluence score** to proceed.

| Signal | Points | Condition (for CALL entry) | Notes |
|--------|--------|---------------------------|-------|
| EMA Regime | +3 | Price > EMA20 > EMA50 (stacked bullish) | 0 pts in Choppy/RangeBound — that's expected |
| EMA Proximity | +2 | Price within **1.5%** of EMA9 or EMA20 | Widened from 0.5% — captures realistic pullbacks |
| RSI Zone | +2 | RSI between **30–55** (bounce / early uptrend) | Widened from 35–50 |
| MACD Crossover | +2 | MACD line above signal line | |
| MACD Histogram | +1 | Histogram positive | |
| ADX Momentum | +2 | ADX ≥ **20** (any directional momentum) | Lowered from 25 — fires in choppy/transitional markets |
| BBand Extreme | +2 | Price within **2%** of lower Bollinger Band | Widened from 1% |
| Volume Confirm | +1 | Volume ≥ 1.0× 20-day average | |

**Maximum score: 15 points**
**Minimum score for entry: 6 points** (40% of max — at least 3–4 signals must agree)

For PUT entries, conditions are mirrored: EMA bearish, RSI 45–70, MACD below signal, price near upper band.

**Choppy regime:** Previously blocked all trades. Now treated identically to Range-Bound. Iron condors
work well in oscillating, mean-reverting markets. In choppy conditions EMA regime (+3) won't fire, so
the effective ceiling is ~12 pts. A score of 6 still requires BBand extreme + MACD + ADX + volume to
agree — a conservative but realistic setup for a choppy underlying.

### Entry Procedure

```
1. Scan underlying bars → compute all indicators
2. Determine regime (trending/range-bound/choppy)
3. Score all signals — Choppy uses BBand proximity for direction
4. If score < 6 → SKIP (logged to scan_results DB table with outcome=below_threshold)
5. If score >= 6 → proceed to contract selection
7. Fetch options chain from Alpaca
8. Filter contracts by:
   a. Direction: calls (bullish) or puts (bearish)
   b. Delta: 0.30–0.45 (sweet spot for directional with some safety)
   c. DTE: 14–45 days (enough time for thesis to play out, avoid rapid theta burn)
   d. Premium: ask_price * 100 <= max_position_usd
   e. Liquidity: OI >= 100, volume >= 50, spread <= $0.20
   f. IV Rank: 20–60 (avoid overpaying for premium)
9. Rank qualifying contracts by:
   a. Delta closest to 0.35 (preferred sweet spot)
   b. Tightest bid-ask spread
   c. Highest open interest
10. Select top contract → submit limit order at mid-price
```

### Contract Selection Preferences

```
[entry]
min_confluence_score = 8
preferred_delta      = 0.35
delta_min            = 0.30
delta_max            = 0.45
dte_min              = 14
dte_max              = 45
iv_rank_min          = 15
iv_rank_max          = 60
order_type           = "limit"
limit_price_method   = "mid"    # (bid + ask) / 2
```

---

## 6. Exit Logic — Tiered Risk Management

### Exit is where money is made or saved. This section is critical.

The system uses a **three-tier exit framework**:

### Tier 1: Profit Target (Happy Path)

| Condition | Action |
|-----------|--------|
| Unrealized P&L >= +30% of premium paid | Close 50% of position (if >1 contract) |
| Unrealized P&L >= +50% of premium paid | Close remaining position |
| If only 1 contract: P&L >= +40% | Close entire position |

**Rationale:** Taking profits at 30–50% of premium is the quant standard for long options. Holding for "home runs" dramatically reduces win rate with a small account.

### Tier 2: Stop-Loss (Capital Preservation)

| Condition | Action |
|-----------|--------|
| Unrealized P&L <= -30% of premium paid | Close entire position immediately |
| Underlying breaks below EMA50 (for calls) or above EMA50 (for puts) | Close position regardless of P&L |
| Confluence score drops below 4 | Close position (thesis invalidated) |

**Stop-loss is NON-NEGOTIABLE.** The daemon must enforce this mechanically. No manual override. No "let it ride." A 30% loss on a $200 position is $60. A 100% loss is $200. The difference compounds quickly on a $1,000 account.

### Tier 3: Time-Based Decay Management

| Condition | Action |
|-----------|--------|
| DTE remaining <= 10 | Close position regardless of P&L |
| DTE remaining <= 7 AND P&L < +10% | Close immediately (theta is eating alive) |
| Held for 5+ trading days with < 5% move | Close (dead money, redeploy capital) |

**Rationale:** Theta decay accelerates dramatically inside 2 weeks. With our 14–45 DTE entry window, we should never be holding at expiration. If the thesis hasn't played out by day 10, it's not going to.

### Exit Priority (highest to lowest)

```
1. Stop-loss breach (-30%)     → IMMEDIATE close
2. DTE <= 7 with < +10% P&L   → IMMEDIATE close
3. Profit target hit (+50%)    → Close
4. EMA50 break (thesis dead)   → Close
5. Time decay (DTE <= 10)      → Close
6. Dead money (5 days, < 5%)   → Close
7. Confluence score collapse   → Close
```

### PDT-Aware Exit Logic

Before submitting any exit order:

```
1. Check: Was this position opened today?
2. If YES:
   a. Check day_trade_count in rolling 5-day window
   b. If day_trade_count >= max_day_trades_per_5d (2):
      - BLOCK the exit UNLESS:
        i.  Stop-loss at emergency threshold (loss >= emergency_stop_pct 50%) → ALLOW
        ii. Exceptional win (gain >= exceptional_win_pct 75%) → ALLOW (take the money)
      - Otherwise: log warning "PDT limit reached, holding overnight"
      - Set position.force_exit_next_open = true
   c. If day_trade_count < max_day_trades_per_5d:
      - Allow exit, increment counter
      - Log: "Day trade #{n} used"
3. If NO (opened on a prior day):
   - Allow exit normally, does NOT count as day trade
```

**Exceptional win rule:** If an intraday position has gained ≥ 75% of premium paid *and* we still have day trade budget remaining, always take the profit — do not hold overnight to avoid a day trade. A 75%+ same-day gain is unusual and worth spending a day trade on. If the PDT limit is already hit, the position is held overnight and flagged `force_exit_next_open = true`.

```toml
[pdt]
max_day_trades_per_5d  = 2
rolling_window_days    = 5
emergency_stop_pct     = 50.0   # allow same-day close at this loss %
exceptional_win_pct    = 75.0   # allow same-day close at this gain %
block_on_limit         = true
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

| Confluence Score | Position Size |
|-----------------|---------------|
| 8–10 (minimum threshold) | 50% of max_position_usd ($100) |
| 11–14 (moderate confidence) | 75% of max_position_usd ($150) |
| 15+ (high confidence) | 100% of max_position_usd ($200) |

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
min_confluence_score   = 8
preferred_delta        = 0.35
delta_min              = 0.30
delta_max              = 0.45
dte_min                = 14
dte_max                = 45
order_type             = "limit"
limit_price_method     = "mid"

[strategy.exit]
profit_target_partial_pct = 30.0
profit_target_full_pct    = 50.0
profit_target_single_pct  = 40.0
stop_loss_pct             = 30.0
emergency_stop_pct        = 50.0
time_exit_dte             = 10
theta_exit_dte            = 7
theta_exit_min_pnl_pct    = 10.0
dead_money_days           = 5
dead_money_min_pct        = 5.0

# ──────────────────────────────────────
# Regime Detection
# ──────────────────────────────────────
[strategy.regime]
ema_fast               = 9
ema_mid                = 20
ema_slow               = 50
adx_period             = 14
adx_trend_threshold    = 25
adx_no_trend_threshold = 20
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

# Score-based sizing tiers
[[sizing.tiers]]
score_min = 8
score_max = 10
size_factor = 0.50

[[sizing.tiers]]
score_min = 11
score_max = 14
size_factor = 0.75

[[sizing.tiers]]
score_min = 15
score_max = 25
size_factor = 1.00

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
