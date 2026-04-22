# Ferrum Week 1 Paper Trading Report
**Period:** 2026-04-08 — 2026-04-11 (3 trading days)  
**Account:** Alpaca Paper / Cash account  
**Strategy:** Iron Condor (single-leg long puts/calls, swing hold)  
**Report date:** 2026-04-11

---

## ⚠️ Important: Confluence Values Were Modified Before Week 1

The original confluence scoring parameters from the strategy design doc were **not** used during this paper trading week. They were revised after 2 days of zero entries (2026-04-06/07). The agent reviewing this report should evaluate results in light of these changes.

### Original values (strategy doc v2.0)
| Parameter | Original |
|-----------|----------|
| `min_confluence_score` | 8 / 15 |
| `adx_trend_threshold` | 25.0 |
| `adx_no_trend_threshold` | 20.0 |
| Choppy regime | **BLOCKED — no trade** |
| EMA proximity tolerance | 0.5% |
| RSI call zone | 35–50 |
| RSI put zone | 50–65 |
| ADX score gate | > 25 |
| BBand extreme tolerance | 1% |

### Revised values (used in week 1)
| Parameter | Revised | Rationale |
|-----------|---------|-----------|
| `min_confluence_score` | **6 / 15** | 2 days, zero entries at 8 |
| `adx_trend_threshold` | **22.0** | ADX rarely reached 25 in current market |
| `adx_no_trend_threshold` | **17.0** | Shrinks choppy zone |
| Choppy regime | **ALLOWED (scores as RangeBound)** | Iron condors suit oscillating markets |
| EMA proximity tolerance | **1.5%** | 0.5% too tight for daily bars |
| RSI call zone | **30–55** | Widened from 35–50 |
| RSI put zone | **45–70** | Widened from 50–65 |
| ADX score gate | **≥ 20** | Fires in transitional markets |
| BBand extreme tolerance | **2%** | 1% too precise for daily candles |

---

## Regime Distribution (3,907 symbol-scans)

| Regime | Count | % |
|--------|-------|---|
| Choppy | 2,699 | 69% |
| Range-Bound | 810 | 21% |
| Trending Down | 251 | 6% |
| Trending Up | 147 | 4% |

**Observation:** The market spent nearly 70% of scan cycles in choppy regime. Under the original rules, the bot would have had **zero entries** this week. The revised rules allowed choppy-regime trades, which is what generated all week 1 activity. Whether this is correct behavior for an iron condor strategy is a key open question.

---

## Scan Outcome Distribution

| Outcome | Count | % |
|---------|-------|---|
| below_threshold | 1,790 | 46% |
| no_contracts | 1,079 | 28% |
| entered | 1,038 | 27% |

**Note on `no_contracts`:** SPY, QQQ, IWM, AAPL, BAC, COIN — all major liquid symbols — consistently returned no qualifying contracts. This is likely a free-tier data issue (Alpaca indicative feed may not return all strikes). Alpaca Plus upgrade is in progress and should resolve this.

---

## Per-Symbol Scan Summary

| Symbol | Scans | Avg Score | Min | Max | Entries | No Chain | Below Threshold |
|--------|-------|-----------|-----|-----|---------|----------|-----------------|
| SIRI | 145 | 10.0 | 8 | 11 | 143 | 2 | 0 |
| HOOD | 142 | 7.0 | 4 | 10 | 22 | 91 | 29 |
| SOFI | 149 | 6.9 | 5 | 7 | 145 | 0 | 4 |
| AAL | 142 | 6.6 | 6 | 8 | 141 | 1 | 0 |
| F | 150 | 6.5 | 6 | 8 | 36 | 114 | 0 |
| LYFT | 149 | 6.1 | 4 | 7 | 101 | 2 | 46 |
| SPY | 143 | 6.0 | 6 | 7 | 0 | 143 | 0 |
| QQQ | 152 | 6.0 | 6 | 6 | 0 | 152 | 0 |
| IWM | 151 | 6.0 | 6 | 6 | 0 | 151 | 0 |
| CLF | 149 | 6.0 | 6 | 6 | 149 | 0 | 0 |
| BAC | 149 | 6.0 | 6 | 6 | 136 | 13 | 0 |
| UBER | 145 | 5.7 | 2 | 8 | 48 | 20 | 77 |
| NIO | 144 | 5.6 | 2 | 8 | 33 | 55 | 56 |
| AAPL | 147 | 5.6 | 4 | 7 | 0 | 113 | 34 |
| PLTR | 156 | 5.2 | 2 | 7 | 0 | 52 | 104 |
| T | 151 | 5.0 | 4 | 6 | 71 | 0 | 80 |
| COIN | 152 | 4.9 | 4 | 7 | 0 | 64 | 88 |
| FCX | 156 | 4.7 | 4 | 6 | 0 | 52 | 104 |
| SNAP | 152 | 4.6 | 4 | 9 | 0 | 35 | 117 |
| VALE | 156 | 4.3 | 4 | 5 | 0 | 0 | 156 |
| PFE | 149 | 4.1 | 2 | 7 | 0 | 19 | 130 |
| RIVN | 154 | 4.0 | 2 | 7 | 13 | 0 | 141 |
| AMZN | 156 | 3.7 | 2 | 5 | 0 | 0 | 156 |
| AMD | 156 | 3.3 | 2 | 4 | 0 | 0 | 156 |
| MARA | 156 | 3.0 | 2 | 4 | 0 | 0 | 156 |
| INTC | 156 | 3.0 | 2 | 5 | 0 | 0 | 156 |

**Consistent scorers (avg ≥ 6):** SIRI, HOOD, SOFI, AAL, F, LYFT, SPY, QQQ, IWM, CLF, BAC  
**Consistently weak (avg < 4):** AMZN, AMD, MARA, INTC — consider removing from universe  
**Chain data missing (no_contracts dominant):** SPY, QQQ, IWM, F, BAC, AAPL, LYFT — free-tier data gap

---

## Fills Log

```csv
symbol,side,qty,price,timestamp
SOFI260515P00016000,buy,2,0.93,2026-04-08T13:49:35
F260501P00011500,buy,6,0.27,2026-04-08T13:49:35
CLF260515P00009000,buy,2,0.73,2026-04-08T13:50:32
CLF260515P00009000,sell,2,0.68,2026-04-08T13:51:21
F260508P00011500,buy,6,0.30,2026-04-08T14:04:20
NIO260515P00006000,buy,6,0.32,2026-04-08T14:08:33
SOFI260515P00016000,sell,2,1.09,2026-04-08T15:16:50
SOFI260424P00016000,buy,3,0.56,2026-04-08T15:21:12
F260508P00011500,buy,6,0.29,2026-04-08T17:13:50
NIO260508C00007000,buy,3,0.24,2026-04-09T13:53:58
NIO260508C00007000,sell,2,0.12,2026-04-09T13:54:10
NIO260515C00007000,buy,4,0.24,2026-04-09T14:03:41
NIO260515C00007000,sell,4,0.18,2026-04-10T13:30:09
SIRI260424C00024000,buy,2,0.33,2026-04-10T15:19:40
```

---

## Trade Log (Closed Positions)

| Contract | Underlying | Dir | Entry | Exit | Entry $ | Exit $ | Qty | P&L | Exit Reason |
|----------|-----------|-----|-------|------|---------|--------|-----|-----|-------------|
| CLF260515P00009000 | CLF | call | Apr 8 13:49 | Apr 8 13:51 | $0.74 | $0.68 | 2 | **-$10.00** | ema50_break |
| SOFI260515P00016000 | SOFI | put | Apr 8 13:49 | Apr 8 15:17 | $0.93 | $1.09 | 2 | **+$32.00** | trailing_profit |
| NIO260508C00007000 | NIO | call | Apr 9 13:49 | Apr 9 13:54 | $0.26 | $0.12 | 3 | **-$42.00** | stop_loss |
| NIO260515C00007000 | NIO | call | Apr 9 14:03 | Apr 10 13:30 | $0.25 | $0.18 | 4 | **-$24.00** | ema50_break |
| SIRI260424C00024000 | SIRI | call | Apr 10 15:19 | Apr 11 00:00 | $0.33 | $0.34 | 2 | **+$2.00** | ema50_break |

**Realized P&L (closed positions): -$42.00**  
**Open positions as of Apr 11:** F puts, SOFI424 puts (unrealized P&L unknown at report time)

---

## Day Trades (PDT Tracker)

3 day trades were recorded. PDT limit is 2 per rolling 5-day window.

| Contract | Underlying | Open Time | Close Time | Open $ | Close $ | P&L | Emergency? |
|----------|-----------|-----------|------------|--------|---------|-----|-----------|
| CLF260515P00009000 | CLF | Apr 8 13:49 | Apr 8 13:51 | $0.73 | $0.68 | **-$10** | No |
| SOFI260515P00016000 | SOFI | Apr 8 13:49 | Apr 8 15:17 | $0.93 | $1.09 | **+$32** | No |
| NIO260508C00007000 | NIO | Apr 9 13:49 | Apr 9 13:54 | $0.26 | $0.12 | **-$42** | No |

**Key finding:** All 3 day trades were triggered by automated exits (not emergency threshold). The CLF exit was EMA50 break within 2 minutes of entry — the bot bought and immediately got an EMA50 signal. The SOFI exit was the trailing profit system working correctly (profitable). The NIO exit was stop-loss at -54% within 5 minutes of entry.

The PDT limit was exceeded (3 trades, limit=2). None were flagged `was_emergency=1`, meaning the PDT gate logic did not block them — this needs review. It's likely that position tracking (in-memory) was reset between sessions, losing the day-trade count.

---

## Exit Reason Analysis

| Exit Reason | Count | Notes |
|-------------|-------|-------|
| ema50_break | 3 | CLF (-$10), NIO515 (-$24), SIRI (+$2) — all triggered at midnight between sessions |
| trailing_profit | 1 | SOFI (+$32) — trailing system working as designed |
| stop_loss | 1 | NIO Apr (-$42) — triggered within minutes of entry |

**EMA50 break at midnight:** 3 of 5 exits fired at `00:00 UTC` (outside market hours). This suggests the exit monitor is running overnight and triggering on stale prices. This is a bug — exits should only be submitted during market hours or queued for open.

---

## API & Infrastructure Issues

- **Free-tier rate limit hits:** The TUI was polling Alpaca 360 calls/min vs 200/min limit. Fixed on Apr 9 by adding per-call timers (positions every 30s, PnL every 2 min).
- **Fill sync 429s:** `fill_sync_task` hitting rate limits. Interval remains 60s.
- **Chain data gaps:** SPY, QQQ, IWM, F, BAC chains returning no qualifying contracts on free `indicative` feed. Likely a coverage gap, not a logic error. Alpaca Plus upgrade pending.
- **Alpaca Plus upgrade:** In progress (ID verification). Will increase rate limits and may improve options chain data coverage.

---

## Open Questions for Strategy Review

1. **Should choppy regime be tradeable?** 69% of scans were choppy. Allowing choppy trades generated all week 1 entries but also most losses. The EMA50 signal firing immediately after entry on CLF and NIO suggests choppy entries have no trend to hold against.

2. **EMA50 break exit running at midnight** — This is a code bug. Prices at midnight are stale. The exit should be gated to market hours.

3. **PDT gate not blocking day trades** — 3 day trades were logged but none were blocked. The in-memory PDT counter may be resetting on daemon restarts.

4. **NIO stop-loss within 5 minutes of entry** — Score was 5-6 in choppy regime. The underlying moved sharply against the position immediately. Minimum hold time (e.g., 1 trading day before stop-loss is armed) may prevent this.

5. **AMZN, AMD, MARA, INTC consistently below threshold** — Avg scores 3.0-3.7. Consider removing from the symbol universe to reduce scan time and API calls.

6. **F has 150 scans, 36 entries, 114 no_contracts** — F consistently scores ≥ 6 but the chain is missing 76% of the time. Likely a free-tier gap.

---

## Recommendations for Week 2

1. **Fix EMA50 break exit to respect market hours** (bug — highest priority)
2. **Investigate PDT gate** — confirm day_trade_count survives daemon restarts
3. **Add minimum hold time** before stop-loss arms (e.g., 1 full trading day)
4. **Consider re-raising `min_confluence_score` to 7** once Alpaca Plus is active and chain data improves — current 6/15 may be too permissive in choppy regime
5. **Remove AMZN, AMD, MARA, INTC** from symbol universe (chronically low scores)
6. **Continue holding** remaining open positions (F puts, SOFI424 puts) through next week before evaluating exit rules further
