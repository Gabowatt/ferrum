# ferrum — session checkpoint

## Completed this session

- [x] Full config schema overhaul — symbols tiers, liquidity, entry/exit, regime, IV engine, sizing, PDT
- [x] Indicators engine — EMA 9/20/50, RSI 14, MACD (12/26/9), ADX 14, Bollinger Bands, ATR 14, HV20, volume ratio
- [x] Regime detection — TrendingUp / TrendingDown / RangeBound / Choppy
- [x] Confluence scoring — 11 signals, minimum score 8 gate
- [x] PDT tracker — rolling 5-day window, emergency stop + exceptional win (≥75%) exceptions
- [x] IV rank engine — HV proxy on startup, switches to actual IV after 30 days of snapshots
- [x] Iron Conduit strategy — full scan loop: bars fetch → indicators → regime → confluence → options chain → delta/DTE/liquidity/IV/budget filters → contract ranking → signal
- [x] DB schema extended — day_trades, iv_snapshots, trade_log tables
- [x] Risk guard updated — equity floor, drawdown, position limits, cash reserve, sector cap
- [x] Updated README with strategy summary
- [x] Exceptional-win day trade rule added to strategy doc and implemented in PDT tracker

## Next immediate steps

### 1. TUI — PDT status panel
Show PDT state directly in the TUI header or positions panel:
- Day trades used / allowed in rolling window (e.g. "PDT: 1/2")
- Color: green = budget available, yellow = 1 used, red = limit reached
- Add `get_pdt` IPC command → daemon returns `{ used: n, max: n, resets_on: date }`

### 2. Wire exit monitoring loop
- Add `exit_monitor_task` in daemon — runs every `exit_check_interval` (60s)
- Fetch open positions from Alpaca `/v2/positions`
- For each open position: fetch current quote, compute unrealized P&L %
- Evaluate all exit conditions (profit target, stop-loss, DTE, dead money)
- PDT-aware exit: call `pdt.check_exit_allowed()` before submitting close order
- Log all exit decisions to TUI and trade_log table

### 3. Order submission
- Implement actual limit order submission via Alpaca `POST /v2/orders`
- Track order status (PENDING → FILLED / CANCELLED / EXPIRED)
- On fill: write to fills + trade_log, update PDT tracker if day trade

### 4. IPC — positions panel data
- Add `get_positions` IPC command → fetch from Alpaca and return to TUI
- TUI positions panel currently shows placeholder — wire to live data

### 5. Market hours gate
- Daemon should check Alpaca `/v2/clock` before each scan
- Skip scan if market is closed or outside scan_start_time / scan_end_time window

## To run locally

```bash
# config.toml must exist with your paper keys — see docs/ferrum-iron-conduit-strategy.md §13
cargo run -p ferrum-daemon   # terminal 1
cargo run -p ferrum-tui      # terminal 2
```
