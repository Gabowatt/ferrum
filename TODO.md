# ferrum — current checkpoint

> Historical log of completed work: [`docs/changelog/history.md`](docs/changelog/history.md)

## Status

- **Active branch**: `V2.1` (multi-strategy refactor) — Phase 1 + Phase 2 complete. UX polish landed (toggle bug fix, hide-PnL parrot, header ticker, left-column stretch). Ready for Phase 3 Iron Condor.
- **Last shipped**: V2 web dashboard + tuning fixes merged to `main`.
- **Last paper run**: 2026-04-22 — 0 entries again. Hoping tomorrow gets a fill so the new dashboard (strategy stats, badges, ticker) actually has data to show.
- **Build**: clean, zero warnings (`cargo build --workspace`).
- **Daemon**: stop button verified working; live-mode hard block removed (gated only at startup via `live.enabled`).

## 🐛 Active bugs

_None open._

## Next up — V2.1 multi-strategy architecture

> Full design doc: [`docs/multi-strategy-plan.md`](docs/multi-strategy-plan.md)

Promote the daemon from one hardcoded strategy to a registry of strategies that
run in parallel with live UI toggles. Rename the current strategy
(misleadingly called "iron conduit") to **Forge**, then add a true 4-leg
**Iron Condor** as the second strategy.

### Phase 1 — Strategy registry + attribution (no behavior change)
Make the daemon multi-strategy-shaped while it still runs only Forge.
- [x] Rename `IronConduitStrategy` → `ForgeStrategy`; renamed strategy doc + config section + log prefixes.
- [x] DB migration: `strategy_id` (default 'forge') on fills/trade_log/scan_results; nullable `position_id` on trade_log (Phase 3 leg grouping). Idempotent ALTER via PRAGMA inspection.
- [x] Promote `Strategy` trait with `id()`; add `StrategyHandle { id, scan_interval, enabled, strategy }` and `build_strategies(&AppConfig)` factory.
- [x] `AppState.strategies: Vec<Arc<StrategyHandle>>`; IPC `Start` spawns one supervisor loop per handle. `Stopping → Idle` coordinated via `active_strategy_loops` AtomicUsize counter.
- [x] Add `strategy_id` to `OpenPositionMeta`.
- [x] Pipe `strategy_id` through order submission → trade log writes (entry, close-pending, close-confirmed). Fills row tagging deferred — Alpaca activities come back without an originating strategy and the DB-side `DEFAULT 'forge'` keeps Phase 1 correct; will revisit in Phase 2 with an order_id → strategy_id map.
- [ ] (Phase 2 prep) `Strategy::check_exit` — deferred until Iron Condor lands and needs strategy-specific exits.

### Phase 2 — Live toggle + UI plumbing
- [x] `enabled: AtomicBool` per strategy handle; loop checks before each scan (shipped in Phase 1 Commit C; seed value now read from `[strategies.<id>].enabled` on boot).
- [x] IPC `GetStrategies`, `SetStrategyEnabled`. Persistence uses `toml_edit` so comments + ordering in config.toml survive the rewrite. End-to-end tested via raw socket: socket toggle → AtomicBool flip → config rewrite → next GetStrategies reflects state.
- [x] Web `StrategiesPanel` with toggles + per-strategy mini-stats (open positions, signals today, scans today). Disabled strategy rows still poll, so re-enabling shows immediate stats.
- [x] Strategy badge column on `PositionsPanel`. Legacy / manually-opened positions render as `manual` (gray) so the eye doesn't have to pattern-match missing data.
- [x] **Post-Phase-2 UX polish**:
  - StrategiesPanel toggle: optimistic UI + visible inline error if the daemon round-trip fails (root cause of "toggle won't turn off" was silent error swallowing).
  - PnlPanel hide toggle in the header — when off, swaps the body for an animated party parrot (frames sourced verbatim from hugomd/parrot.live, hue-cycled per frame). Preference persists in `sessionStorage`.
  - Header ticker strip — Nasdaq-style scrolling marquee between PDT counter and Start/Stop. New `IpcCommand::GetTickerSnapshot` hits Alpaca `/v2/stocks/snapshots` for the entire scan universe; web polls `/api/ticker` every 15s. Hover pauses the scroll.

### Phase 3 — Iron Condor strategy
**Manual prerequisite:** request multi-leg spread approval on Alpaca paper.
- [ ] Multi-leg `mleg` order support in `orders.rs`.
- [ ] `strategy/iron_condor.rs` — 4-strike selection by delta.
- [ ] `IronCondorEntryConfig` — `short_delta`, `wing_width_pct`, `min_credit_pct_of_width`.
- [ ] Condor sizing (max loss = wing × 100 − credit).
- [ ] Strategy-specific exits via `Strategy::check_exit`: 50% PT / 2× credit stop / 21 DTE close.
- [ ] `OpenPositionMeta` evolves to optional multi-leg.
- [ ] Web `PositionsPanel` collapses 4 legs into one row.

### Backlog (post V2.1)
- Run Forge for a week with the 0.25 ATR veto and re-evaluate near-miss data.
- Vertical credit spreads as a third strategy.
- Per-strategy P&L tiles in the dashboard.
- **StrategiesPanel — fixed footprint as the registry grows.** Today every
  strategy adds another row; with 3+ strategies the panel will dominate the
  right column. Convert the panel body to a single-strategy view with
  prev/next arrows (or a small dot pager / carousel). Keep the aggregate
  `N/M enabled` meta in the header so the operator can still see overall
  state at a glance. The toggle and per-strategy stats stay where they are
  inside the active card; arrows persist the selected strategy in
  `sessionStorage` so a reload doesn't reset to the first one.
- Homelab deployment (systemd/Docker, LAN-only CORS, persistent data volume).

## To run

```bash
cargo run -p ferrum-daemon      # terminal 1 — leave running
cargo run -p ferrum-web         # terminal 2 — Axum server on :3000
cd web && npm run dev           # terminal 3 — Vite dev server, opens browser

# Production: ferrum-web serves the built React bundle directly
cd web && npm run build
cargo run -p ferrum-web         # dashboard on http://localhost:3000
```
