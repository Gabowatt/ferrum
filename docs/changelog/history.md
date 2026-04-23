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

## Cleanup pass — 2026-04-21

- [x] Removed `.github/workflows/deploy.yml` (Pages deploy) — homelab plan instead.
- [x] Removed legacy config fields: `chain_scan_interval`, `min_confluence_score`,
      `profit_target_partial_pct`, `profit_target_full_pct`. Removed matching
      `StrategyConfig` / `ExitConfig` / `EntryConfig` fields.
- [x] Removed dead daemon code: `cancel_order` (never called), `Strategy::name`
      trait method (never called), unused struct fields on `AlpacaOrder`,
      `AlpacaPosition.unrealized_pl`, `OptionContract.contract_type`,
      `AlpacaClock.next_close`, `IvRankResult.{current_iv, method}`, and the
      now-unused `IvMethod` enum.
- [x] Build now compiles with **zero warnings**.
- [x] Removed `ferrum-export` crate (tax/CSV tooling, never used).
- [x] Dropped now-unused `clap` and `anyhow` from workspace deps.
- [x] Updated README architecture diagram (TUI/Phase-2 split → React + Axum + daemon).
- [x] Renamed strategy directory tree references; dropped `ferrum-export/` from workspace docs.
- [x] Archived 175 lines of historical session checkpoints from `TODO.md` into
      `docs/changelog/history.md`. TODO went from 263 → ~70 lines.
- [x] `.gitignore`: added `.claude/worktrees/`.

## V2.1 planning — 2026-04-21

- [x] Multi-strategy architecture plan written: `docs/multi-strategy-plan.md`.
- [x] Naming settled: current strategy renames to **Forge**; new 4-leg credit
      spread becomes **Iron Condor**.
- [x] Design decisions settled: shared risk budget, write-back toggles to
      config.toml, real ALTER TABLE migration, per-strategy scan intervals.
- [x] TODO restructured around the V2.1 phased delivery (registry → toggle UI
      → Iron Condor strategy).

## V2.1 Phase 1 — Forge rename (2026-04-21)

- [x] `IronConduitStrategy` → `ForgeStrategy` in `crates/ferrum-daemon/src/strategy.rs`
      (struct, impl, instantiation in `run_strategy_loop`).
- [x] All `[iron-conduit]` log prefixes → `[forge]` (16 sites).
- [x] `config.toml`: `[strategy] name = "iron-conduit"` → `"forge"`.
- [x] `docs/ferrum-iron-conduit-strategy.md` → `docs/ferrum-forge-strategy.md`
      (git-tracked rename); title + codename + §13 config example updated.
- [x] README: strategy section retitled "Forge — Multi-Regime Long Options v2.2";
      file references updated; multi-strategy-plan.md added to docs tree.
- [x] Build verified clean (zero warnings).

## V2.1 Phase 1 — DB migration for strategy_id (2026-04-21)

- [x] `db.rs::migrate()` now adds `strategy_id TEXT NOT NULL DEFAULT 'forge'`
      to `fills`, `trade_log`, and `scan_results`, plus nullable `position_id`
      to `trade_log` (Phase 3 will use it to group condor legs).
- [x] Fresh DBs get the columns via the `CREATE TABLE` bodies; existing DBs
      get them via an idempotent `ALTER TABLE` pass that inspects
      `PRAGMA table_info` first (no SQLite "duplicate column" errors on
      reboot).
- [x] Migration SQL verified against a copy of the live `ferrum.db`
      (10,764 existing scan_results rows backfilled to 'forge' as expected).
- [x] Writers unchanged in this commit — defaults keep them working. A later
      commit threads an explicit `strategy_id` parameter through them.

## V2.1 Phase 1 — Strategy registry + multi-loop supervisor (2026-04-21)

- [x] `Strategy` trait now exposes `id() -> &'static str` (used as the
      `strategy_id` tag in DB rows + in IPC payloads).
- [x] New `StrategyHandle { id, scan_interval, enabled: AtomicBool, strategy }`
      wraps `Arc<dyn Strategy>` for the supervisor. `enabled` is reserved for
      Phase 2's live-toggle IPC; for now it always starts true.
- [x] `strategy::build_strategies(&AppConfig) -> Vec<Arc<StrategyHandle>>` is
      the single source of truth for which strategies the daemon hosts. Phase 1
      ships only Forge; Phase 3 adds Iron Condor here.
- [x] `AppState.strategies` carries the registry; `AppState.active_strategy_loops`
      (`AtomicUsize`) lets multiple supervisor tasks coordinate the
      `Stopping → Idle` transition without races (last loop out flips the bit).
- [x] `run_strategy_loop` now takes `Arc<StrategyHandle>` + `Arc<AppState>`,
      logs every event with the handle's `id`, and gates each cycle on
      `handle.is_enabled()` so Phase 2 only has to flip the flag.
- [x] IPC `Start` iterates `state.strategies` and spawns one supervisor task
      per handle. `Stop` is unchanged — the existing `stop_notify` ping wakes
      every loop simultaneously.
- [x] Behavior preserved: with one registered strategy (Forge) the runtime
      shape matches V2 exactly — same scan cadence, same logs (now prefixed
      `[forge]` already from Commit A), same risk + PDT path.
- [x] Build clean, zero warnings.

## V2.1 Phase 1 — strategy_id threaded through writers (2026-04-21)

- [x] `OpenPositionMeta` gains `strategy_id: &'static str` (first field). The
      static lifetime falls out of `Strategy::id()` returning `&'static str`,
      so we never allocate per-position just to remember which strategy
      opened it.
- [x] `db::insert_scan_result` and `db::insert_trade_log` take `strategy_id`
      explicitly. `insert_trade_log` also takes `position_id: Option<&str>`
      now — Phase 1 always passes `None`; Phase 3 will use it to group the
      four legs of an Iron Condor under one synthetic position id.
- [x] Strategy scan loop binds `let strategy_id = self.id();` once at the top
      of `scan()` and passes it to all six `insert_scan_result` callsites
      (every veto + the eventual signal). One source of truth per scan cycle.
- [x] `submit_signal_orders(...)` takes `strategy_id` and stamps it onto the
      new `OpenPositionMeta` and the `insert_trade_log` open-row write — so
      every entry row in `trade_log` is attributed without relying on the
      DB-side default.
- [x] `exit_monitor.rs` and `order_poller.rs` close-side `insert_trade_log`
      calls forward `meta.strategy_id` (and `None` for `position_id`).
- [x] Fills (Alpaca activities API) keep the DB-side `DEFAULT 'forge'`. Adding
      explicit per-fill attribution needs an `order_id → strategy_id` lookup
      map — deferred to Phase 2 alongside the IPC live-toggle work.
- [x] Build clean, zero warnings. Phase 1 of the multi-strategy plan is
      complete; runtime behavior is identical to V2 but the daemon is now
      shaped for N strategies.

## V2.1 Phase 2 — Live strategy toggles + UI plumbing (2026-04-22)

End-to-end live enable/disable for strategies, persisted to config.toml,
plumbed all the way to a React `StrategiesPanel`. Daemon still hosts only
Forge; the same wiring will light up automatically when Iron Condor lands
in Phase 3.

- [x] **IPC contract**: new `IpcCommand::GetStrategies` returns a
      `Vec<StrategyInfo>` with `id`, `enabled`, `scan_interval_secs`,
      `open_positions`, `signals_today`, `scans_today`.
      `IpcCommand::SetStrategyEnabled { id, enabled }` flips the live
      AtomicBool and persists to config.toml.
- [x] **Config persistence**: optional `[strategies.<id>] enabled = bool`
      table (`#[serde(default)]`, so existing configs load unchanged).
      `build_strategies(&AppConfig)` consults the map on boot; missing
      entries default to enabled. Toggle persistence uses `toml_edit`
      (not the line-based rewrite that handles `mode = "..."`) so comments
      and section ordering in config.toml survive the rewrite.
- [x] **Stats source**: new `Database::scan_tally_today(strategy_id) →
      (total, entered)` query — counts scan_results rows from UTC midnight
      filtered by `strategy_id`. Fast enough to call per `GetStrategies`
      request; no caching needed at the current scale.
- [x] **Position attribution**: `Position` IPC type gains a
      `strategy_id: Option<String>` field. The IPC `fetch_positions`
      handler reads `OpenPositionMeta::strategy_id` from the in-memory
      map; positions Alpaca returns that we don't have meta for (manual
      positions, restart-orphans) render as `null` → "manual" badge in
      the UI.
- [x] **Web backend**: `GET /api/strategies` and
      `POST /api/strategies/:id/enabled` proxied to the daemon.
- [x] **React `StrategiesPanel`**: rows show the strategy badge,
      scan-interval, open positions, signals today, scans today, and a
      toggle switch. Per-row pending flag disables only the toggled row
      while the request is in flight.
- [x] **`PositionsPanel` strategy column**: small chip per position so
      the operator can tell at a glance which strategy each line item
      came from. Color per strategy id (Forge = orange, Iron Condor =
      purple in Phase 3, manual = gray).
- [x] **`useDashboard` hook**: strategies fetched on mount + every 15s,
      and `refresh.strategies` exposed so the panel can re-poll
      immediately after a successful toggle.
- [x] **End-to-end smoke test**: raw `nc -U /tmp/ferrum.sock` confirms
      the round trip — `GetStrategies` returns enabled=true, toggle off
      writes `[strategies.forge] enabled = false` to disk, next
      `GetStrategies` returns enabled=false, toggle on restores. Comments
      in config.toml verified intact after rewrites.
- [x] Build clean (`cargo build --workspace` + `npm run build` in `web/`),
      zero warnings.

## V2.1 dashboard polish — 2026-04-22

Three quality-of-life passes after Phase 2 testing surfaced rough edges.

- [x] **StrategiesPanel toggle bug — root-caused & fixed.** The Forge
      toggle never visually flipped off in the browser, even though the
      raw-socket Phase 2 test was green. Two compounding causes:
  - `crates/ferrum-web/src/main.rs` registered the route as
    `/api/strategies/{id}/enabled`. axum 0.7 only adopted `{}` param
    syntax in 0.8 — in 0.7 it's a literal segment, so the POST never
    matched the API router and fell through to the static-file
    fallback (GET-only) → 405 Method Not Allowed. Fixed by switching
    to `:id` and dropping a comment so a future axum bump doesn't
    regress it.
  - `web/src/components/StrategiesPanel.tsx` had no optimistic UI and
    silently `console.error`'d failures. Even when the route was
    eventually fixed, a slow round-trip looked like a no-op. Now the
    toggle flips locally on click, an in-flight `pending` flag
    disables only the toggled row, and any API failure reverts the
    optimistic flip and surfaces an inline red `toggle failed: …`
    string under the row. Errors stop being mysteries.
- [x] **Hide-PnL parrot.** New toggle in the PnlPanel header (mirrors
      the strategy-toggle styling, floats right via
      `.panel-header > .toggle-switch { margin-left: auto }`). When off,
      the panel body swaps for a `<ParrotAnimation>` — the 10 frames
      under `web/src/parrot/{0..9}.txt` are the unmodified ASCII files
      from `hugomd/parrot.live`, loaded via Vite's `?raw` import (so the
      source-of-truth is the original .txt files; zero risk of leading-
      whitespace drift). Cycles at 70 ms with a 36°/frame hue step so a
      full rainbow completes once per parrot loop. Preference persists
      in `sessionStorage` (intentional — survives F5 but not a fresh
      browser session). `web/src/vite-env.d.ts` added with `?raw` module
      declaration so TypeScript stops complaining.
- [x] **Header ticker strip.** Nasdaq-style scrolling marquee between
      the PDT counter and Start/Stop. New
      `IpcCommand::GetTickerSnapshot` calls Alpaca
      `/v2/stocks/snapshots?symbols=…&feed=iex` once for the entire
      `cfg.symbols.all()` universe — `latestTrade.p` for price,
      `prevDailyBar.c` for the day-change baseline. New
      `IpcResponse::TickerSnapshot { entries: Vec<TickerEntry> }` and
      `TickerEntry { symbol, price, change_pct }` in
      `ferrum-core/types.rs`. New `/api/ticker` route, `getTicker()` in
      `web/src/api.ts`, polled every 15 s by `useDashboard`. Failures
      log to `console.warn` only — the strip is decorative and a
      transient Alpaca hiccup shouldn't redline the dashboard. The
      `<TickerStrip>` renders two stitched copies of the tape inside a
      `.ticker-track` that animates `translateX(-50%)` over 60 s; copy
      #2 lines up exactly where copy #1 started → seamless loop with no
      jump. Edge mask fades items in/out, hover pauses the animation.
      Up/down/flat coloring per item.
- [x] **Left-column panels fill leftover height.** `.main-grid` now
      claims `flex: 1 1 auto; min-height: 0` (was `align-items: start`
      which sized columns to content), `.left-column > .panel` becomes a
      `flex: 1 1 0` flex column, and each panel body
      (`.positions-table-wrap` / a new matching `.fills-table-wrap`) is
      `flex: 1 1 0; min-height: 0; overflow-y: auto`. Result: Positions
      and Recent Fills evenly share the available vertical gap and each
      table scrolls independently when overstuffed. No more dead
      whitespace below either box.
- [x] Dropped the "party parrot" caption beneath the ASCII art —
      operator preference; the bird carries itself.
- [x] Build clean (`cargo build --workspace` + `npm run build`),
      zero warnings. Commits: `9817da3` (UX additions),
      `2bb2e6d` (axum 0.7 fix + caption + stretch).

