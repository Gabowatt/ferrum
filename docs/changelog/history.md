# ferrum — changelog

Append-only log of completed work, newest sections at the bottom.
Forward-looking work lives in `/TODO.md`.

## Initial scaffold

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

## Completed this session (paper trading day 3 — 2026-04-08)

- [x] Fixed TUI PnL panel: was showing 1M cumulative as "today", month/year hardcoded 0.0
  - today = equity[-1] - equity[-2] from 1M daily history
  - month = profit_loss[-1] from 1M query (cumulative)
  - year  = profit_loss[-1] from 1A query (best-effort, falls back to 0)
- [x] Removed [E] export keybinding from TUI (keybindings bar + help overlay)

## Next session — resume here (Friday 2026-04-11)

## Completed this session (week 2 prep — 2026-04-11)

- [x] Bug fix: EMA50 break exit now gated to market hours (9:30–16:05 ET) — was firing at midnight on stale prices
- [x] Bug fix: min_hold_hours = 8.0 added — stop-loss won't arm until position held 8h; emergency stop (-50%) bypasses gate
- [x] Bug fix: was_emergency correctly recorded in day_trades DB (was always false)
- [x] Bug fix: emergency_stop added as explicit exit reason (was previously just stop_loss at -50%)
- [x] Week 2 tuning: min_confluence_score raised 6 → 7 (week 1 showed too many weak choppy entries)
- [x] Week 2 tuning: removed AMZN, AMD, MARA, INTC, VALE from symbol universe (avg score 3.0–3.7)
- [x] PDT bug (false alarm): PDT correctly blocked at limit; NIO was allowed through as emergency stop (-54% > 50% threshold)

## Completed this session (v2.1 strategy implementation + cleanup — 2026-04-11)

- [x] Added v2.1 EntryConfig fields: trend_min_score, range_min_score, choppy_min_score, allow_choppy,
      extreme_proximity_atr, cooldown_after_close_hours, bb_width_min_pct, ema_slope_lookback_bars
- [x] Rewrote confluence_score with regime-specific signal sets:
      - Trend (max 12): EMA9/20 touch, RSI zone, MACD hist inflection, higher-low/lower-high, volume contraction, ADX strength
      - Range/Choppy (max 10): band touch, RSI extreme, reversal candle, distance from mean, volume spike
- [x] Updated detect_regime to require +DI/-DI direction, EMA20 slope, BB width ≥ 5% for RangeBound
- [x] Added BarContext struct (high, low, open, 5d extremes, 20d extremes, MACD hist prev)
- [x] Added extreme proximity veto — rejects call if high within 0.5 ATR of 20d high, put if low near 20d low
- [x] Added cooldown veto — no new entries in same underlying within 4h of closing a position
- [x] Regime-specific sizing: trend 7-8=0.5×, 9-10=0.75×, 11-12=1.0×; range 6=0.5×, 7-8=0.75×, 9-10=1.0×
- [x] AppState: added last_close_by_underlying for cooldown tracking
- [x] order_poller.rs: records close timestamp on confirmed fill
- [x] Removed old ferrum-iron-conduit-strategy.md (v2.0); renamed _v2.md → ferrum-iron-conduit-strategy.md (v2.1 is now canonical)
- [x] Updated README with v2.1 strategy summary: three-stage gate, regime table, entry thresholds, exit priority table

## Completed this session (V1 sign-off + v2.2 — 2026-04-20)

- [x] Week 2 data analysis: 6,856 scans, 23 entries (0.34%), realized **+$78** (LYFT +$127, SOFI −$42, SIRI −$7)
- [x] v2.2 strategy tuning: `trend_min_score` 7 → 6 (HOOD trending_down avg 6.16 was the most-missed setup)
- [x] Symbol pruning: removed PFE (range avg 0.8) and PLTR (range avg 0.31)
- [x] Strategy doc bumped to v2.2 with week-2 changelog entry
- [x] README updated: symbol universe, threshold table, file list
- [x] Week 2 report written: `docs/week-2-report-2026-04-20.md`
- [x] Alpaca Algo Trader Plus rate-limit profile applied:
    - `market_data_cooldown` 4s → 0s
    - TUI poll intervals: positions/fills 30s → 10s, PnL 120s → 30s
    - Daemon `fill_sync_task` 60s → 15s
    - Stripped legacy free-tier comments
- [x] V1 merged to main

## Completed this session (V2 web app — 2026-04-21)

- [x] Removed ferrum-tui crate entirely (ratatui/crossterm deps dropped from workspace)
- [x] Created ferrum-web crate — Axum HTTP server on port 3000
  - All IPC commands forwarded via REST (status, pnl, positions, fills, pdt, clock, logs, equity)
  - SSE `/api/stream` endpoint — polls daemon every 2s, broadcasts new LogEvents to clients
  - `POST /api/mode` — writes mode to config.toml, returns restart_required: true
  - Serves built React files from `web/dist/` via ServeDir
  - CORS permissive — allows GitHub Pages origin
- [x] Created `web/` — React 18 + TypeScript + Vite dashboard
  - Tokyo Night dark theme (CSS variables, all custom CSS — no UI libraries)
  - Header: gradient logo, pulsing status dot, mode chip, market status, PDT counter
  - Positions panel: CALL/PUT badges, P&L bar (green/red gradient, ±50% cap)
  - P&L panel: today/month/year stat cards + Recharts equity area chart
  - Fills panel: recent fills with BUY/SELL badges, time-ago
  - Log stream: SSE live logs, level-colored badges, auto-scroll (500-entry ring buffer)
  - Mode switch: Paper↔Live toggle with confirm dialog + restart-required banner
  - Controls: Start/Stop in header
- [x] Updated daemon: removed V1 live-trading hard block; ToggleMode writes config.toml
- [x] Added `GetEquityHistory` IPC command + daemon handler (Alpaca portfolio history)
- [x] GitHub Actions workflow: `.github/workflows/deploy.yml` — builds and deploys to Pages on push to main
- [x] README updated with new quickstart, dev mode, live trading instructions

## Completed this session (V2 follow-ups + data-driven tune — 2026-04-21)

- [x] **Stop button fix** — added `stop_notify: Arc<Notify>` to AppState; IPC `Stop`
      pings it and strategy loop `tokio::select!`s sleep vs notify, so Stop is
      instant instead of waiting out the 60s scan interval. UI now fast-polls
      status for 10s after Stop click to surface the Running → Stopping → Idle
      transition inside the 5s default polling interval.
- [x] **Market close indicator** — verified fix (displays `clock.next_change` directly)
- [x] **Extreme proximity veto tuned** — 0.5 ATR was blocking 100% of SIRI trending_up
      threshold hits today (40 vetoes, 0 entries). Dropped to 0.25 ATR so
      legitimate trend-continuation near-highs get through, while literal
      at-the-extreme climax entries are still blocked.
- [x] **Sector concentration tracking wired** — added `[symbols.sectors]` map in
      config; `RiskGuard::with_open_underlyings()` builds sector counts from open
      positions; `check_entry` blocks new entries whose sector is already at
      `max_sector_positions` (currently 2).
- [x] **Live-mode hard block removed from risk.rs** — was still rejecting all live
      entries (stale V1 gate). Live is now gated only at daemon startup via
      `live.enabled`, matching the stated V2 design.
- [x] **Chain data gaps investigated** — week 2 was 0.7%, today 0.14% — not a
      systemic problem. Small improvement: when Alpaca returns `null` OI we now
      pass the contract through instead of treating it as 0 (the bid/ask spread
      check in the snapshot step is the real liquidity gate). Also added context
      to the `no_contracts` log line (total returned, DTE range, OI min).
- [x] **TODO "To run" section updated** — dropped `ferrum-tui`, added the
      `ferrum-web` + Vite dev-server workflow.

