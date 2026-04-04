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
    ├── ferrum-build-plan.md           # phase-by-phase build plan
    └── ferrum-iron-conduit-strategy.md  # full strategy specification
```

The build plan and strategy doc in `docs/` are the authoritative references. Each Claude Code session starts by reading the relevant doc alongside `git log --oneline -10` and `TODO.md`.

## Strategy: Multi-Regime Confluence (`iron-conduit`)

> Full specification: [`docs/ferrum-iron-conduit-strategy.md`](docs/ferrum-iron-conduit-strategy.md)

### Overview

A **probability-weighted, multi-signal confluence system** designed for a $1,000 cash account. Not a directional gamble — every entry requires multiple independent indicators to agree before a contract is selected.

### How it works

1. **Regime detection** — classifies each symbol as Trending Up, Trending Down, Range-Bound, or Choppy using EMA9/20/50 and ADX. Choppy = no trade.
2. **Confluence scoring** — 11 signals scored across EMA alignment, RSI zone, MACD crossover, ADX strength, Bollinger Band position, and volume. **Minimum score of 8 required to proceed.**
3. **Contract selection** — fetches live options chain from Alpaca, filters by delta (0.30–0.45), DTE (14–45 days), premium budget (≤$200), liquidity (OI ≥100, volume ≥50, spread ≤$0.20), and IV rank (≤60).
4. **Position sizing** — scales by confluence score (50% / 75% / 100% of max) and IV rank.
5. **Exit management** — tiered exits: profit target (40–50% gain), stop-loss (30% loss), time decay (DTE ≤10), and dead-money close (5 days with <5% move).

### PDT protection

Cash account, max **2 day trades per rolling 5-day window**. Same-day exits are only allowed if:
- Loss ≥ 50% of premium (emergency stop), or
- Gain ≥ 75% of premium (exceptional win — take the money), or
- Day trade budget is not yet used

Otherwise positions are held overnight and flagged for exit at next open.

### Symbol universe

| Tier | Symbols | Condition |
|---|---|---|
| 1 | SPY, QQQ, IWM | Always scan |
| 2 | F, SOFI, PLTR, NIO, RIVN, HOOD, SNAP, AAL, CLF, T, PFE, BAC, INTC | Always scan |
| 3 | AMD, AMZN, AAPL, MARA, COIN | Only when IV rank ≥ 40 |

### Target performance

- Win rate: 55–65% | Avg winner: +20–40% | Avg loser: -15–25%
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
