# Multi-strategy architecture plan

> **Status:** Planning. Target branch `V2.1`. No code changes have landed yet.
> **Naming chosen:** current strategy → **Forge**; new 4-leg credit spread → **Iron Condor**.

The current daemon runs one hardcoded strategy that buys long single-leg
calls/puts. The naming "iron conduit" is misleading — the code is not actually
an iron condor. This plan upgrades the daemon to host multiple strategies in
parallel, with live UI toggles to enable/disable each one, and adds a true
4-leg iron condor as the second strategy.

---

## 1. Naming

| Today's code | Renamed to | Rationale |
|---|---|---|
| `IronConduitStrategy` (long calls/puts on confluence + regime) | **`Forge`** | Themed (iron/metalworking), short, distinct from "Iron Condor". |
| (new) 4-leg defined-risk credit spread | **`Iron Condor`** | The name finally matches what the code does. |

`Forge` was picked over alternatives (`Anvil`, `Spark`, `Bellows`, `Confluence`)
because:
- Short and easy to say.
- Doesn't clash with existing concepts (`Confluence` is the name of Forge's
  scoring system, so reusing it as the strategy name would be confusing).
- Sits naturally next to `Iron Condor` in the UI.

### Rename surface (Phase 1 grep map)
- File: `crates/ferrum-daemon/src/strategy.rs` → split into `strategy/mod.rs` + `strategy/forge.rs` (and later `strategy/iron_condor.rs`).
- Type: `IronConduitStrategy` → `ForgeStrategy`.
- Doc: `docs/ferrum-iron-conduit-strategy.md` → `docs/ferrum-forge-strategy.md`.
- Config: `[strategy]` → `[strategies.forge]` (see §3).
- Logs: any `[iron-conduit]` log prefix → `[forge]`.
- README: strategy section heading and overview.

---

## 2. Current architecture (single-strategy)

```
config.toml [strategy]   ──►  AppState.config.strategy
                              │
                              ▼
                         IronConduitStrategy::new()  (hardcoded in run_strategy_loop)
                              │
                              ▼
                         scan → signals → submit → OpenPositionMeta(contract → meta)
                                                          │
                                                          ▼
                                                    exit_monitor (over all positions)
```

Risk guard, PDT, sector cap, exit monitor, sizing all assume **one strategy,
one set of positions**. Nothing in the DB or in `OpenPositionMeta` knows which
strategy opened a given position.

---

## 3. Target architecture (multi-strategy)

```
config.toml
  [strategies]
    [strategies.forge]
      enabled = true
      scan_interval_secs = 60
      [strategies.forge.entry]   ...
      [strategies.forge.exit]    ...
    [strategies.iron_condor]
      enabled = false
      scan_interval_secs = 300
      [strategies.iron_condor.entry]   ...
      [strategies.iron_condor.exit]    ...

  [risk]                          # shared portfolio budget (unchanged)
  [pdt]                           # shared (unchanged — PDT is account-wide)
                              │
                              ▼
AppState.strategies: Vec<Arc<StrategyHandle>>
  └─ StrategyHandle {
       id: &'static str,
       enabled: AtomicBool,
       scan_interval: Duration,
       strategy: Arc<dyn Strategy>,
     }
                              │
                              ▼
For each handle: tokio::spawn(run_strategy_loop(handle, state))
  loop checks `enabled` before each scan; honors per-strategy interval
                              │
                              ▼
Submitted orders tagged with strategy_id
  → OpenPositionMeta gains `strategy_id: &'static str`
  → fills/trade_log/scan_results tables gain `strategy_id` column
                              │
                              ▼
exit_monitor stays global, but dispatches strategy-specific exit rules
  by reading `meta.strategy_id` (Forge: trailing stop; Condor: 50% PT / 21 DTE)
risk_guard stays global; learns about per-strategy cap (config-driven)
                              │
                              ▼
IPC adds:
  • GetStrategies → [{ id, enabled, status, open_count, today_pnl }]
  • SetStrategyEnabled { id, enabled } → live toggle (writes back to config.toml)
                              │
                              ▼
Web UI:
  • New "Strategies" panel: list with toggle switches + per-strategy stats
  • Positions panel: add strategy badge column (FORGE | IRON CONDOR)
```

### Sketch: `Strategy` trait

```rust
#[async_trait::async_trait]
pub trait Strategy: Send + Sync {
    fn id(&self) -> &'static str;
    fn scan_interval(&self) -> Duration;
    async fn scan(&self, state: &AppState) -> Result<Vec<Signal>, FerrumError>;
    /// Strategy-specific exit logic. Called per position by exit_monitor.
    async fn check_exit(
        &self,
        state: &AppState,
        meta: &OpenPositionMeta,
        ctx: &ExitContext,
    ) -> Option<ExitReason>;
}
```

Phase 1 keeps `check_exit` empty for Forge (existing global exit_monitor logic
runs as today); Phase 3 wires Iron Condor's exits through it.

### Sketch: `StrategyHandle`

```rust
pub struct StrategyHandle {
    pub id:            &'static str,
    pub enabled:       AtomicBool,
    pub scan_interval: Duration,
    pub strategy:      Arc<dyn Strategy>,
}

impl StrategyHandle {
    pub fn is_enabled(&self) -> bool { self.enabled.load(Ordering::Relaxed) }
    pub fn set_enabled(&self, v: bool) { self.enabled.store(v, Ordering::Relaxed); }
}
```

### Sketch: per-strategy loop

```rust
async fn run_strategy_loop(handle: Arc<StrategyHandle>, state: Arc<AppState>) {
    loop {
        // Bot-level Stop still wins (existing notify-based stop)
        select! {
            _ = state.stop_notify.notified() => return,
            _ = sleep(handle.scan_interval) => {}
        }
        if !handle.is_enabled() { continue; }
        if !market_is_open(&state).await { continue; }
        match handle.strategy.scan(&state).await {
            Ok(signals) => for sig in signals {
                /* ... risk_guard.check_entry(handle.id, &sig) ... */
                submit_signal_orders(&state, handle.id, &sig).await;
            },
            Err(e) => log error,
        }
    }
}
```

### DB migration (Phase 1)

```sql
ALTER TABLE fills         ADD COLUMN strategy_id TEXT NOT NULL DEFAULT 'forge';
ALTER TABLE trade_log     ADD COLUMN strategy_id TEXT NOT NULL DEFAULT 'forge';
ALTER TABLE scan_results  ADD COLUMN strategy_id TEXT NOT NULL DEFAULT 'forge';
-- Phase 3 adds for multi-leg grouping:
ALTER TABLE trade_log     ADD COLUMN position_id TEXT;  -- nullable; condor legs share an id
```

We're early V2 with a wiped DB, so we can either run the ALTERs or just wipe
again. Decision in §6.

---

## 4. Phased delivery

### Phase 1 — Strategy registry + attribution (no behavior change)
The point of phase 1 is to make the daemon multi-strategy-shaped while it still
runs only Forge. No new strategies yet. Forge keeps trading exactly as today.

- [ ] Promote `Strategy` trait: add `id()`, `scan_interval()`, `check_exit()` (no-op for Forge).
- [ ] Replace single `IronConduitStrategy::new()` call site with `Vec<Arc<StrategyHandle>>`.
- [ ] Spawn one strategy loop per registered strategy.
- [ ] Add `strategy_id: &'static str` to `OpenPositionMeta`.
- [ ] DB migration: add `strategy_id` column to `fills`, `trade_log`, `scan_results`.
- [ ] Pipe `strategy_id` through `submit_signal_orders` → fill records → trade log.
- [ ] Rename `IronConduitStrategy` → `ForgeStrategy`. Update strategy doc, README, config section.

**Acceptance:** daemon starts, runs Forge, opens/closes positions exactly as
today, but every record in DB carries `strategy_id = 'forge'`. UI is unchanged.

### Phase 2 — Live toggle + UI plumbing
Add user-facing controls without adding the new strategy.

- [ ] Add `enabled: AtomicBool` per handle. Loop checks before each scan.
- [ ] New IPC: `GetStrategies`, `SetStrategyEnabled`.
- [ ] `SetStrategyEnabled` writes `enabled = true/false` back to `config.toml` under the right `[strategies.<id>]` section.
- [ ] Web: new `StrategiesPanel` component with toggles + per-strategy mini-stats.
- [ ] Add strategy badge column to `PositionsPanel`.
- [ ] Optional: per-strategy P&L tile (cheap once attribution is in DB).

**Acceptance:** clicking the toggle live-pauses/resumes a strategy without
restarting the daemon. Positions display correct strategy badge.

### Phase 3 — Iron Condor strategy
Now Iron Condor lives as a self-contained module alongside Forge.

**Manual prerequisite:** request multi-leg spread approval on Alpaca paper
account. Without it the `mleg` order endpoint returns 403. Can be requested in
parallel with Phase 1.

- [ ] `orders.rs` — multi-leg `mleg` order class with 4 legs, each carrying `ratio_qty`, `side`, `position_intent`.
- [ ] `strategy/iron_condor.rs` — 4-strike selection (short call + long call wing + short put + long put wing) by delta.
- [ ] `IronCondorEntryConfig`: `short_delta` (~0.20), `wing_width_pct` (~5%), `min_credit_pct_of_width`.
- [ ] Sizing — max loss = (wing_width × 100) − credit; new `size_condor()` separate from `size_long()`.
- [ ] Entry logic — only fires in `range_bound` regime (to start). Phase 4+ could add directional vertical spreads in trending regimes.
- [ ] Strategy-specific exits via `Strategy::check_exit`: 50% max profit / 2× credit stop / 21 DTE mechanical close.
- [ ] DB `trade_log.position_id` — group the 4 legs of one condor (added in Phase 1's migration so it's pre-baked).
- [ ] `OpenPositionMeta` evolves to optionally hold a `legs: Vec<LegMeta>` (Forge stays single-leg via `legs.len() == 1`).
- [ ] Web `PositionsPanel` — collapse 4 legs of a condor into one row showing short/long strikes.

**Acceptance:** with both strategies enabled and Alpaca approval in place, the
daemon can simultaneously open Forge long contracts and Iron Condor 4-leg
positions, exit each by its own rules, and report both in the UI.

---

## 5. What this plan does not cover (intentionally)

- **Backtesting harness** — would be valuable but is a bigger orthogonal project.
- **Hot-reload of strategy entry params** (changing `short_delta` without restart). Out of scope; restart works.
- **Manual close from UI** ("close position now" button). Punt to later.
- **Per-strategy risk caps** (e.g. "Iron Condor can use max 3 of 4 position slots"). Start with shared budget; add caps later if one strategy starves the other.
- **More than one bot instance** — still one daemon process, one Alpaca account.

---

## 6. Decisions settled

| Decision | Choice | Notes |
|---|---|---|
| Names | `Forge` + `Iron Condor` | See §1. |
| Risk budget | **Shared** | All strategies compete for the same `max_open_positions`, `max_portfolio_risk_pct`, sector caps. Add per-strategy caps later if needed. |
| Toggle persistence | **Write back to config.toml** | Mirrors the existing mode toggle. Survives daemon restart. |
| DB migration | **Real ALTER TABLE migration** | Cheap with sqlx, keeps any historical scan data we accumulate during Phase 1. (Wipe is the fallback if it gets fiddly.) |
| Per-strategy intervals | **Per-strategy** | Forge: 60s. Iron Condor: 300s (5min — slower-moving setups). Configured under each `[strategies.<id>]`. |

---

## 7. Effort estimate

| Phase | Sessions | Risk |
|---|---|---|
| 1 — registry + attribution + rename | 1 | Schema migration + IPC contract changes; touches many files but each change is small. |
| 2 — live toggle + UI panel | 0.5–1 | Low; mostly UI + one new IPC pair. |
| 3 — Iron Condor strategy | 2 | Medium; new order class, new sizing model, strategy-specific exit dispatch. Blocked on Alpaca approval. |
| **Total** | **3.5–4** | |

Forge keeps trading unchanged through phases 1–2, so paper P&L data continues
accumulating during the refactor.

---

## 8. Open follow-ups (post Phase 3)

- Vertical credit spreads as a third strategy (defined-risk directional, complements Iron Condor's neutrality).
- Per-strategy P&L charts in the dashboard (today/month/year already cheap once attribution is in DB).
- Strategy-level kill switches in `risk.rs` (e.g. "auto-disable Iron Condor for the day if it's down 3 condors in a row").
