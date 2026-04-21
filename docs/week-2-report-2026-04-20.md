# Ferrum Week 2 Paper Trading Report
**Period:** 2026-04-13 — 2026-04-20 (5 trading days)  
**Account:** Alpaca Paper / Cash account (upgrading to Algo Trader Plus)  
**Strategy:** Iron Conduit v2.1 (multi-regime, regime-specific signal sets, vetoes)  
**Report date:** 2026-04-20  
**Status:** ✅ V1 sign-off — strategy went net positive; merging to main.

---

## Headline

**Realized P&L (week 2): +$78** (vs. −$42 in week 1).

| Trade | Direction | Entry → Exit | Net |
|-------|-----------|--------------|-----|
| LYFT 260515P00014000 ×10 | put (range-bound) | 0.86–0.92 → 1.02 | **+$127** (trailing_profit) |
| SOFI 260522P00015000 ×1 | put (trending_down) | 0.89 → 0.47 | −$42 (stop_loss) |
| SIRI 260515C00025000 ×2 | call (trending_up, day trade) | 0.485 → 0.45 | −$7 (ema50_break) |

The LYFT trade is what we wanted v2.1 to do: scan it 213 times in range_bound regime, only enter when the score crosses 6, ride the trailing-profit ladder until the gap closes. Worked exactly as designed.

---

## Scan & entry distribution (6,856 scans across 21 symbols)

### Regime breakdown

| Regime | Scans | % |
|--------|-------|---|
| Choppy | 4,758 | 69.4% |
| Range-Bound | 1,280 | 18.7% |
| Trending Up | 683 | 10.0% |
| Trending Down | 135 | 2.0% |

The 70% choppy share is essentially unchanged from week 1. v2.1's `allow_choppy = false` correctly translates this into "no trade" rather than the implicit choppy entries that drove most of week 1's losses.

### Outcome breakdown

| Outcome | Count | % |
|---------|-------|---|
| choppy (rejected) | 4,758 | 69.4% |
| below_threshold | 2,026 | 29.6% |
| no_contracts | 49 | 0.7% |
| **entered** | **23** | **0.34%** |

### Entries by regime

| Regime | Scans | Entered | Conversion |
|--------|-------|---------|------------|
| Range-Bound | 1,280 | 19 | 1.5% |
| Trending Up | 683 | 3 | 0.4% |
| Trending Down | 135 | 1 | 0.7% |

Note: the 19 range_bound entries are dominated by repeated LYFT signals (same setup, fired across multiple scan ticks until size cap hit). Distinct trades: 3.

The vetoes (extreme proximity, cooldown) **never appear in the outcome stats** — meaning either nothing came close enough to trigger them, or the vetoes are correctly silent in the absence of edge cases. Either way they're not the cause of the low entry rate.

---

## Per-symbol scoring (week 2)

Avg score per symbol within their non-choppy regime (highest first):

| Symbol | Regime | Avg | Max | Entries | Verdict |
|--------|--------|-----|-----|---------|---------|
| HOOD | trending_down | **6.16** | 9 | 0 | Best trend symbol — missed because threshold was 7 |
| IWM | trending_up | 5.32 | 6 | 0 | Consistent but never reaches 7 |
| SIRI | trending_up | 5.19 | 7 | 3 | Only trend symbol that crossed |
| SOFI | trending_down | 5.17 | 6 | 0* | One distinct trade (range-bound score path) |
| QQQ | trending_up | 5.00 | 6 | 0 | Same shape as IWM |
| T | trending_down | 5.00 | 5 | 0 | Pinned at 5 |
| FCX | trending_up | 4.00 | 4 | 0 | Pinned at 4 |
| SPY | trending_up | 4.00 | 4 | 0 | Pinned at 4 |
| BAC | trending_up | 3.84 | — | 0 | — |
| UBER | range_bound | 2.98 | 6 | 0 | Hits 6 occasionally |
| LYFT | range_bound | 2.90 | — | 19 | Distinct trade: 1 — the +$127 winner |
| AAPL | range_bound | 2.54 | 4 | 0 | Below range threshold |
| RIVN | range_bound | 2.27 | 5 | 0 | — |
| PFE | range_bound | **0.80** | 2 | 0 | Cut for week 3 |
| PLTR | range_bound | **0.31** | 2 | 0 | Cut for week 3 |

Symbols that classified 100% choppy across the week (no scoring opportunity): SNAP, NIO, F, COIN, CLF, AAL.

---

## What v2.1 fixed (vs. week 1)

| Week 1 problem | Week 2 outcome |
|----------------|----------------|
| 27% of scans entered, mostly in choppy | Dropped to 0.34%; choppy is now hard-blocked |
| Flat baseline scores (everyone 6.0) | Per-symbol/per-regime averages now span 0–6.16 |
| EMA50 break exits firing at midnight | Market-hours gate held — no overnight false exits |
| Stop-loss firing minutes after entry | `min_hold_hours = 8.0` blocked premature stops |
| PDT counter resetting between sessions | DB-backed — only 1 day trade recorded (SIRI, correctly logged) |

---

## What v2.1 broke

**Trend score floor of 7 is too aggressive.** HOOD trending_down averaged 6.16 across 56 scans and never crossed 7. IWM/QQQ/SPY trending_up plateaued around 4–5. The signal set is producing scores that are *consistent* (no longer flat at 6.0) but the bar for action is set above where the signals actually live.

The realized win came from range-bound (LYFT). The realized losses came from a trend trade (SOFI stop) and a day trade (SIRI EMA50). With trend entries throttled this hard, we're effectively a single-regime (range-bound) bot — which is fine for now but loses the multi-regime thesis.

---

## v2.2 changes

### Strategy
- `entry.trend_min_score`: **7 → 6** — captures HOOD, IWM, QQQ trend signals that consistently sit just below the old gate.
- Sizing tier table updated to match: trend 6–8 → 0.5×, 9–10 → 0.75×, 11–12 → 1.0×. (The code already did this; only the docs needed re-aligning.)
- Symbol universe: removed **PFE** (range avg 0.8) and **PLTR** (range avg 0.31). All other tier-2 names retained.
- Range and choppy thresholds left untouched — both behaved correctly in week 2.

### Infrastructure (Alpaca Algo Trader Plus active — 10k req/min)
- `strategy.market_data_cooldown`: **4s → 0s** (the per-symbol scan throttle is no longer needed).
- TUI poll intervals tightened:
  - Positions: 30s → 10s
  - Fills:     30s → 10s
  - PnL:       2 min → 30s
- Daemon `fill_sync_task`: 60s → 15s.
- Removed legacy comments referring to free-tier limits.

### Docs
- Strategy doc bumped to v2.2 with a Week-2 changelog entry.
- README symbol universe and threshold tables updated to match.

---

## Open positions at report time

None. LYFT closed at 12:04 ET on 2026-04-20.

---

## V1 sign-off — what's in main as of this merge

- Daemon, TUI, IPC, SQLite persistence, order submission/poller
- Iron Conduit v2.2 strategy (three-stage gate: vetoes → regime → score)
- PDT tracker, IV rank engine, risk guard
- Tiered exit ladder with market-hours gate and minimum hold time
- Algo Trader Plus rate-limit profile applied
- Two weeks of paper trading data, net positive

V2 work (Axum HTTP layer, web client, multi-leg condors) starts from a clean main.
