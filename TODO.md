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

## Completed this session (V1 paper trading readiness)

- [x] TUI — PDT status in header (green/yellow/red, PDT: used/max)
- [x] Exit monitoring loop — tiered exits: stop-loss, DTE≤7, profit target, time exit, dead money
- [x] Order submission — `orders.rs` submits limit orders via Alpaca POST /v2/orders
- [x] Open position tracking — `OpenPositionMeta` in `AppState.open_positions`
- [x] IPC GetPositions + GetPdt commands
- [x] TUI positions panel wired to live /v2/positions data
- [x] Market hours gate — checks /v2/clock + scan_start/scan_end ET window
- [x] PDT-aware exit blocking with force_exit_next_open flag

## Remaining / nice-to-have

- [ ] Staged profit exits at +30% / +50% for multi-contract positions
- [ ] EMA50 break exit condition (requires fresh bar fetch in exit monitor)
- [ ] Order fill status polling (PENDING → FILLED / CANCELLED)
- [ ] Sector concentration risk tracking
- [ ] Export fills to CSV from TUI [E] keybinding

## To run locally

```bash
# config.toml must exist with your paper keys — see docs/ferrum-iron-conduit-strategy.md §13
cargo run -p ferrum-daemon   # terminal 1
cargo run -p ferrum-tui      # terminal 2
```
