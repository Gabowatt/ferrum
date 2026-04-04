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
- Paper/live switch happens inside the daemon; clients just send the toggle command
- All external calls go through the daemon only — never from the TUI directly
- Options chain data comes directly from Alpaca (no Polygon dependency)

## Workspace structure

```
ferrum/
├── Cargo.toml              # workspace root
├── config.toml             # gitignored — your local keys + params
├── crates/
│   ├── ferrum-core/        # shared types, traits, errors
│   ├── ferrum-daemon/      # core background service
│   ├── ferrum-tui/         # ratatui frontend
│   └── ferrum-export/      # tax/CSV export tooling
└── ferrum-build-plan.md    # full phase-by-phase build plan
```

The build plan (`ferrum-build-plan.md`) is the authoritative reference for every milestone, commit convention, dependency choice, and session checkpoint protocol. Each Claude Code session starts by reading it alongside `git log --oneline -10` and `TODO.md`.

## Quickstart

1. Create `config.toml` with your Alpaca paper keys (see the `[alpaca.paper]` section below).
2. Run the daemon in one terminal:
   ```
   cargo run -p ferrum-daemon
   ```
3. Run the TUI in a second terminal:
   ```
   cargo run -p ferrum-tui
   ```

The daemon runs in the background — closing the TUI does not stop it. Send `SIGINT` (`Ctrl-C`) to the daemon process to shut it down cleanly.

## Strategy: Delta Scan (`delta-scan`)

> **Current status:** signals are logged only — no orders are submitted in V1 until the strategy is validated on paper.

### What it does

On every scan interval (default: 30s), the strategy fetches the live options chain for each configured symbol directly from Alpaca and looks for call contracts that meet the entry criteria. Any contract that passes all filters generates an `EnterLong` signal, which is logged to the TUI and passed through the risk guard before any future order submission.

### Entry filters

| Filter | Default | Config key | Description |
|---|---|---|---|
| Delta min | `0.30` | `strategy.delta_scan.delta_min` | Minimum delta — excludes far OTM contracts |
| Delta max | `0.50` | `strategy.delta_scan.delta_max` | Maximum delta — excludes deep ITM contracts |
| DTE min | `7` | `strategy.delta_scan.dte_min` | Minimum days to expiration |
| DTE max | `45` | `strategy.delta_scan.dte_max` | Maximum days to expiration |
| Quote | required | — | Contracts with no bid/ask are skipped |

**Direction:** calls only (long bias). Puts and multi-leg spreads are not yet implemented.

**Order type:** limit at mid-price `(bid + ask) / 2`.

**Quantity:** 1 contract per signal (1 signal per qualifying contract per scan).

### What it does NOT do yet

- No exit logic — no stop-loss, profit target, or time-based close
- No IV filter — raw IV is available from Alpaca but IV rank/percentile requires historical baseline
- No position sizing beyond the fixed qty=1 per signal
- No deduplication — the same contract can signal again on the next scan if still in range
- No order execution in V1 — signals are observed/logged only

### Risk guard (always runs before any future order)

| Guard | Default | Config key |
|---|---|---|
| Max position size | $1,000 | `risk.max_position_usd` |
| Daily drawdown | 2% | `risk.daily_drawdown_pct` |
| Max open legs | 4 | `risk.max_open_legs` |
| Live trading | hard block | `alpaca.live.enabled` |

### Config reference

```toml
[strategy]
symbols            = ["SPY", "QQQ", "AAPL"]  # symbols to scan
scan_interval_secs = 30                        # how often to run

[strategy.delta_scan]
delta_min = 0.30   # ~30-delta call — ATM-ish
delta_max = 0.50   # ~50-delta call — near ATM
dte_min   = 7      # avoid gamma risk below 1 week
dte_max   = 45     # standard 1-2 month window

[risk]
max_position_usd   = 1000   # max USD per position (options notional × 100)
daily_drawdown_pct = 2.0    # halt if down >2% on the day
max_open_legs      = 4      # max simultaneous option legs
```

### Suggested improvements to discuss

These are the most obvious gaps to address before live trading — raise any you want to prioritize:

- **Exit strategy** — profit target (e.g. 50% of premium), stop loss (e.g. 2× premium paid), or DTE-based close (e.g. exit at 21 DTE)
- **IV filter** — only enter when IV rank is above a threshold (e.g. >30th percentile) to avoid buying expensive premium
- **Direction bias** — add put scanning, or make direction conditional on a trend filter (e.g. price above/below 20-day EMA)
- **Spread strategies** — vertical spreads (defined risk), iron condors, or covered calls instead of naked long calls
- **Deduplication** — track open positions and skip contracts already held
- **Position sizing** — scale qty based on account size and risk per trade rather than fixed qty=1

## Safety

Live trading is **disabled** in V1. The daemon will refuse to start in live mode regardless of config.
