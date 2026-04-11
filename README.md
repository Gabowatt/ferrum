# ferrum

Quant-level options trading bot + TUI, powered by Alpaca Trading.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  Phase 1                                                            │
│                                                                     │
│   ┌─────────────────┐      IPC (unix socket)     ┌──────────────┐  │
│   │   ferrum-tui    │ ◄──────────────────────── ►│              │  │
│   │  ratatui · TUI  │                            │              │  │
│   └─────────────────┘                            │              │  │
│                                                  │  ferrum-     │  │
│   ┌─────────────────┐                            │  daemon      │  │
│   │  config.toml    │ ──────────────────────────►│              │  │
│   │  keys · params  │                            │  ┌─────────┐ │  │
│   └─────────────────┘                            │  │Strategy │ │  │
│                                                  │  │ engine  │ │  │
│   ┌─────────────────┐                            │  ├─────────┤ │  │
│   │   local DB      │ ◄─────────────────────── ► │  │  State  │ │  │
│   │ SQLite · fills  │                            │  │ manager │ │  │
│   └─────────────────┘                            │  ├─────────┤ │  │
│                                                  │  │  Risk   │ │  │
│                                                  │  │  guard  │ │  │
│                                                  │  ├─────────┤ │  │
│                                                  │  │   IPC   │ │  │
│                                                  │  │ server  │ │  │
│                                                  └──┴────┬────┘ │  │
│                                                          │       │  │
│                                                          ▼       │  │
│                                                 ┌──────────────┐ │  │
│                                                 │  Alpaca API  │ │  │
│                                                 │ paper ↔ live │ │  │
│                                                 │ + options    │ │  │
│                                                 │   data       │ │  │
│                                                 └──────────────┘ │  │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│  Phase 2 (V2 — future)                                             │
│                                                                     │
│   ┌─────────────────┐    REST (hosted anywhere)  ┌──────────────┐  │
│   │   Web app       │ ◄─────────────────────────►│  Axum HTTP   │  │
│   │ remote config   │                            │  API layer   │  │
│   └─────────────────┘                            └──────┬───────┘  │
│                                                         │           │
│                                                    connects to      │
│                                                    daemon IPC       │
└─────────────────────────────────────────────────────────────────────┘
```

**Key design decisions:**
- The daemon runs independently — TUI and (eventually) web app are just clients
- Closing the TUI does **not** stop the bot
- All external calls go through the daemon only — never from the TUI directly
- Options chain data comes directly from Alpaca (no Polygon dependency)

## Workspace structure

```
ferrum/
├── Cargo.toml              # workspace root
├── config.toml             # gitignored — your local keys + params
├── crates/
│   ├── ferrum-core/        # shared types, traits, indicators, errors
│   ├── ferrum-daemon/      # core background service
│   ├── ferrum-tui/         # ratatui frontend
│   └── ferrum-export/      # tax/CSV export tooling
└── docs/
    ├── ferrum-build-plan.md              # phase-by-phase build plan
    ├── ferrum-iron-conduit-strategy.md   # full strategy specification (v2.1)
    └── week-1-report-2026-04-11.md       # paper trading week 1 debrief
```

The build plan and strategy doc in `docs/` are the authoritative references. Each Claude Code session starts by reading the relevant doc alongside `git log --oneline -10` and `TODO.md`.

## Strategy: Multi-Regime Iron Condor v2.1

> Full specification: [`docs/ferrum-iron-conduit-strategy.md`](docs/ferrum-iron-conduit-strategy.md)

### Overview

A **probability-weighted, multi-regime confluence system** designed for a $1,000 cash account. Entries require passing three sequential gates — hard vetoes, a positive regime identification, and a regime-specific quality score — before a contract is selected.

### The three-stage gate

```
STAGE 1: VETOES (hard pass/fail)
  → stale bar, IV rank out of range, extreme proximity to 20d high/low,
    within 4h cooldown of closing same underlying
         │ all pass
         ▼
STAGE 2: REGIME CLASSIFICATION (must positively identify)
  → Trending Up:   close > EMA20 > EMA50, ADX ≥ 22, +DI > −DI, EMA20 rising
  → Trending Down: close < EMA20 < EMA50, ADX ≥ 22, −DI > +DI, EMA20 falling
  → Range-Bound:   ADX < 18, BB width ≥ 5%, price within 5% of EMA50
  → Choppy:        everything else → NO TRADE (unless allow_choppy = true)
         │ regime identified
         ▼
STAGE 3: QUALITY SCORING (regime-specific signal set)
  → Trend (max 12): EMA9/20 wick touch, RSI 40–55, MACD inflecting,
                    higher-low structure, volume contraction, ADX strength
  → Range (max 10): band touch, RSI extreme (≤30/≥70), reversal candle,
                    distance from mean, exhaustion volume spike
```

### Entry thresholds

| Regime | Min score | Sizing (score → factor) |
|---|---|---|
| Trending Up / Down | 7/12 | 7–8 → 0.5×  ·  9–10 → 0.75×  ·  11–12 → 1.0× |
| Range-Bound | 6/10 | 6 → 0.5×  ·  7–8 → 0.75×  ·  9–10 → 1.0× |
| Choppy (if enabled) | 8/10 | always 0.5× |

### Contract selection

Fetches live options chain from Alpaca, filters by: delta 0.30–0.45, DTE 14–45 days, premium ≤ $200, OI ≥ 100, volume ≥ 50, spread ≤ $0.20, IV rank ≤ 60. Ranks by delta closest to 0.35, then highest OI.

### Exit management

| Priority | Trigger | Action |
|---|---|---|
| 1 | Loss ≥ 50% (emergency) | Close immediately |
| 2 | Loss ≥ 30% (after 8h hold) | Close |
| 3 | DTE ≤ 7 and P&L < +10% | Close (theta eating premium) |
| 4 | Trailing profit hit | Close (activates at +15%, trails 7% below peak) |
| 5 | EMA50 break (thesis dead) | Close (market hours only) |
| 6 | DTE ≤ 10 | Time exit |
| 7 | 5 days held, < 5% move | Dead money — redeploy |

### PDT protection

Cash account, max **2 day trades per rolling 5-day window**. Same-day exits only allowed for emergency stops (−50%) or exceptional wins (+75%). All other exits held overnight and flagged for next open.

### Symbol universe

| Tier | Symbols | Condition |
|---|---|---|
| 1 | SPY, QQQ, IWM | Always scan |
| 2 | F, SOFI, PLTR, NIO, RIVN, HOOD, SNAP, AAL, CLF, T, PFE, BAC, UBER, LYFT, FCX, SIRI | Always scan |
| 3 | AAPL, COIN | Only when IV rank ≥ 40 |

### Target performance

- Win rate: 55–65% | Avg winner: +20–40% | Avg loser: −15–25%
- Monthly target: 3–8% on capital | Max drawdown tolerance: 10% ($100)

## Quickstart

1. Create `config.toml` with your Alpaca paper keys (see `docs/ferrum-iron-conduit-strategy.md` §13 for the full config reference).
2. Run the daemon:
   ```
   cargo run -p ferrum-daemon
   ```
3. Run the TUI in a second terminal:
   ```
   cargo run -p ferrum-tui
   ```

The daemon runs independently — closing the TUI does not stop it. `Ctrl-C` the daemon to shut it down cleanly.

## Safety

Live trading is **disabled** in V1. The daemon will refuse to start in live mode regardless of config.
