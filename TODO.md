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

## Completed this session (paper trading day 1 — 2026-04-06)

- [x] Diagnosed empty `log_events` table — `log_tx` broadcast had no DB subscriber
- [x] Added log persistence task in `ferrum-daemon/src/main.rs` — subscribes to `log_tx`, writes every event to SQLite via `db.insert_log()`
- [x] Added `db.recent_logs(limit)` query method
- [x] Added `GetLogs { limit }` IPC command + `Logs { events }` IPC response to `ferrum-core/src/types.rs`
- [x] Wired `GetLogs` handler in `ferrum-daemon/src/ipc.rs`
- [x] Restart daemon to activate log persistence
- [x] Fixed EDT/EST bug in scan window check — was hardcoded to UTC-5, now uses UTC-4 Mar–Nov
- [x] COIN bars 404 on IEX feed — expected, gracefully skipped; left in config intentionally
- [x] All symbols 404 on bars/options — root cause: bars + options snapshot calls were hitting paper-api.alpaca.markets instead of data.alpaca.markets; fixed by adding get_data_with_query() to AlpacaClient using DATA_URL constant
- [x] Fixed options chain 404 — wrong endpoint; paper trading options is free on all plans
  - Step 1: GET /v2/options/contracts (Trading API) → filter by DTE, OI, tradable
  - Step 2: GET /v1beta1/options/snapshots?feed=indicative (Data API) → greeks + quotes
- [x] Fixed 429 rate limit — market_data_cooldown was configured but never applied; now sleeps between each symbol in scan loop
- [x] Fixed decode error on SPY/QQQ — open_interest field is a string in API response, not a number; added open_interest_f64() helper

## Completed this session (paper trading day 1 — continued)

- [x] Tokyo Night color scheme in TUI (blue/cyan/green/yellow/orange/red/purple on dark)
- [x] Anvil logo updated to Tokyo Night gradient (cyan → blue → dim)
- [x] Bot log panel now polls GetLogs every 2s — scan summaries visible without entering a position
- [x] Expanded tier2 symbol universe (+10 names); trimmed 5 dead/illiquid tickers (PARA, WBA, X, NOK, GOLD)
- [ ] Single-terminal daemon launch from TUI — deferred, needs more thought

## Completed this session (paper trading day 2 — 2026-04-06)

- [x] Revised confluence scoring — widened all signal thresholds using quant methodology:
  - EMA proximity: 0.5% → 1.5% tolerance (realistic pullback depth)
  - RSI call zone: 35–50 → 30–55; put zone: 50–65 → 45–70
  - ADX score threshold: >25 → ≥20 (fires in choppy/transitional markets)
  - BBand extreme: 1% → 2% tolerance
  - Choppy regime: was blocking all trades → now scores as RangeBound (iron condors thrive in choppy markets)
- [x] Updated config.toml: adx_trend_threshold 25→22, adx_no_trend_threshold 20→17, min_confluence_score 8→6
- [x] Added `scan_results` DB table — every scored symbol logged with regime/score/direction/outcome
- [x] strategy.rs: replaced all `info!()` with `log_tx.send()` so scores are visible in TUI and DB
- [x] Updated strategy doc (Section 3 regime table, Section 5 scoring table + entry procedure)

## Next session — resume here

### Priority 1 — watch paper trading data
Let the bot run with revised scoring and observe:
- Which symbols score ≥ 6 (query `scan_results` table)
- Which outcome buckets dominate: below_threshold / no_contracts / entered
- P&L distribution when positions are entered

### Priority 2 — dynamic / staged profit exits
Design TBD after real P&L data — revisit once positions have been entered:
- Trailing profit target (once P&L > +30%, trail peak by 15%)
- Staged closes for qty > 1 (50% at first target, remainder trails)

### Priority 3 — sector concentration tracking
`RiskGuard::check_entry` has `max_sector_positions` but sector lookup is not wired:
- Add sector map to config or hard-code in risk.rs
- Block entry if open positions in same sector >= max_sector_positions

### Priority 4 — TUI polish (when ready)
- `[E]` export keybinding → write CSV to `~/ferrum-export-YYYY.csv`
- `[P]` privacy toggle — hide PnL values (show `****`)
- `[B]` buying power panel — free cash + used margin

## To run

```bash
cargo run -p ferrum-daemon   # terminal 1 — leave running, press nothing
cargo run -p ferrum-tui      # terminal 2 — press [S] to start strategy
```
