# ferrum — session checkpoint

## Completed (previous sessions)

- [x] Workspace scaffold — Cargo.toml, config.toml, .gitignore
- [x] ferrum-core — AppConfig, AlpacaClient, BotStatus, LogEvent, IPC types, Signal/OptionLeg
- [x] ferrum-daemon — boot, live gate, Alpaca health check, Unix socket IPC, SIGINT/SIGTERM shutdown
- [x] SQLite — fills, log_events, sessions tables
- [x] Full config schema — symbols tiers, liquidity, entry/exit, regime, IV engine, sizing, PDT
- [x] Indicators engine — EMA 9/20/50, RSI 14, MACD, ADX, Bollinger Bands, ATR, HV20, volume ratio
- [x] Regime detection — TrendingUp / TrendingDown / RangeBound / Choppy
- [x] Confluence scoring — 11 signals, minimum score 8 gate
- [x] PDT tracker — rolling 5-day window, emergency stop + exceptional win (≥75%) exceptions
- [x] IV rank engine — HV proxy on startup, switches to actual IV after 30 days of snapshots
- [x] Iron Conduit strategy — full scan loop: bars → indicators → regime → confluence → chain → filters → signal
- [x] DB schema extended — day_trades, iv_snapshots, trade_log tables
- [x] Risk guard — equity floor, drawdown, position limits, cash reserve, sector cap
- [x] Exceptional-win day trade rule — added to strategy doc + implemented

## Completed this session (V1 paper trading readiness + polish)

- [x] Order submission — `orders.rs` submits limit orders via Alpaca `POST /v2/orders`
- [x] Open position tracking — `OpenPositionMeta` in `AppState.open_positions`
- [x] Market hours gate — checks `/v2/clock` + ET scan window (09:45–15:45)
- [x] Exit monitoring loop — tiered exits: stop-loss (-30%), DTE≤7 low-P&L, profit target (+40%), time exit (DTE≤10), dead money (5 days <5%)
- [x] PDT-aware exit — blocks same-day close at limit, allows emergency (-50%) and exceptional win (+75%)
- [x] `force_exit_next_open` flag on PDT-blocked positions
- [x] IPC GetPositions + GetPdt commands
- [x] TUI positions panel — live contract rows with qty, entry price, P&L%
- [x] TUI header — PDT: used/max with green/yellow/red color coding
- [x] ferrum-tui — polls positions and PDT every 500ms loop tick
- [x] Order fill poller — `order_poller.rs` confirms fills every 30s, handles cancels/expirations, records day trades on close
- [x] EMA50 break exit — fetches underlying bars per cycle (cached per underlying), closes call if close < EMA50, put if close > EMA50
- [x] Fixed premature position removal — exit monitor now sets `pending_close_order_id` instead of removing immediately
- [x] Pixel art FERRUM logo — 5-row block-character logo with hot-iron amber→red gradient in TUI header

## Next immediate step — resume here next session

### 1. Dynamic / staged profit exits
Replace the fixed +40% profit target with a smarter exit system:
- **Trailing profit target**: once P&L exceeds a threshold (e.g. +30%), the exit level trails the peak P&L by a fixed offset (e.g. 15%), locking in gains as the contract appreciates
- **Staged closes (qty > 1)**: close 50% at first target, let remainder trail independently
- qty == 1: trailing target only (no split needed)
- Design TBD — revisit after paper trading gives real P&L distribution data

### 2. Sector concentration tracking
`RiskGuard::check_entry` has a `max_sector_positions` config but sector lookup is not wired.
- Add sector map to config or hard-code in risk.rs
- Before entry: count open positions in same sector via `open_positions`
- Block if count >= max_sector_positions

### 3. Export [E] keybinding in TUI

### 4. TUI privacy toggles
- `[P]` key — toggle PnL panel visibility (hide today/month/year values, show `****`)
- `[B]` key — toggle buying power panel in positions area: show free cash + used margin
  (free: available buying power, used: sum of open position market values)
- Useful when screen-sharing or recording
- Wire TUI `[E]` key to call `ferrum-export` binary
- Date range picker modal → write CSV to `~/ferrum-export-YYYY.csv`

## To run for paper trading (Monday)

```bash
# 1. Ensure config.toml has your Alpaca paper keys
cargo run -p ferrum-daemon   # terminal 1 — leave running
cargo run -p ferrum-tui      # terminal 2 — press [S] to start strategy
```

The daemon will:
- Connect to Alpaca paper account
- Wait for market open (09:45 ET)
- Scan every 5 minutes across all symbol tiers
- Submit limit orders at mid-price when confluence score ≥ 8
- Monitor open positions every 60s for exit conditions
- Never exceed 2 day trades in a rolling 5-day window
