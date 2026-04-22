# ferrum — current checkpoint

> Historical log of completed work: [`docs/changelog/history.md`](docs/changelog/history.md)

## Status

- **Branch**: V2 (multi-leg condors + ongoing tuning)
- **Last paper run**: 2026-04-21 — 2,109 scans / 0 entries (extreme_proximity veto blocked the only threshold hit; veto since tuned 0.5 → 0.25 ATR)
- **Build**: clean, zero warnings (`cargo build --workspace`)
- **Daemon**: stop button verified working; live-mode hard block removed (gated only at startup via `live.enabled`)

## 🐛 Active bugs

_None open._

## Next up

### Priority 1 — More paper trading data with the tuned veto
Run V2 for a week and re-evaluate. Specifically watch:
- Did the 0.25 ATR veto unblock SIRI-style trend-continuation trades?
- Did sector concentration tracking actually fire (i.e. block a 3rd entry in a sector)?
- Are the small-cap "always choppy" symbols (F, SOFI, LYFT, UBER, HOOD, NIO,
  RIVN, AAL, CLF, SNAP, COIN) ever leaving choppy regime, or are they dead weight?

### Priority 2 — True multi-leg iron condors
Single-leg directional contracts → 4-leg defined-risk condors.
Estimated 2–3 focused sessions. Full work breakdown:

**Prerequisite (manual):** request multi-leg spread approval on Alpaca paper
account. Without it the multi-leg endpoint returns 403.

**Code work:**
1. `orders.rs` — swap single-leg `POST /v2/orders` for multi-leg
   (`order_class: "mleg"`, `legs: [...]` × 4 with `ratio_qty`, `side`, `position_intent`).
2. `strategy.rs` contract selection — pick 4 strikes (short call + long call wing
   + short put + long put wing) at specified deltas.
3. `EntryConfig` — new fields: `short_delta` (~0.20), `wing_width_pct` (~5%),
   `min_credit_pct_of_width`.
4. Sizing — max loss = (wing width × 100) − credit received; size based on that,
   not premium paid. Different cost model from current `max_position_usd`.
5. Entry logic — condors are *neutral*; current trend-aware scoring doesn't
   apply. Either (a) only trade condors in range_bound regime, or (b) use
   directional vertical spreads in trending regimes and condors in range
   (= two strategies under one roof).
6. Exit monitor — 50% max profit target, 2× credit stop, 21 DTE mechanical close.
7. DB `trade_log` — add `position_id` grouping for the 4 legs.
8. `OpenPositionMeta` — currently keyed by single contract; needs 4-leg structure.
9. Web UI — `PositionsPanel` collapse 4 legs into one row with short/long strike display.

### Priority 3 — Homelab deployment
Long-term: run daemon + web on a homelab box, access dashboard from LAN.
- Decide on supervision (systemd unit vs. Docker)
- LAN-only CORS lockdown
- Persist `data/` (SQLite, logs) on host volume

## To run

```bash
cargo run -p ferrum-daemon      # terminal 1 — leave running
cargo run -p ferrum-web         # terminal 2 — Axum server on :3000
cd web && npm run dev           # terminal 3 — Vite dev server, opens browser

# Production: ferrum-web serves the built React bundle directly
cd web && npm run build
cargo run -p ferrum-web         # dashboard on http://localhost:3000
```
