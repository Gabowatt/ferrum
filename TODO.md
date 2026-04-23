# ferrum — current checkpoint

> Historical log of completed work: [`docs/changelog/history.md`](docs/changelog/history.md)

## Status

- **Active branch**: `V2.1` — wrapping up. One feature still to land
  (the weekly review report), then tagging `v2.1.0`. Target tag is
  after the **2026-04-24** close if the report is in by then; slip a
  few days otherwise — the report has to actually fire on Friday for
  us to trust it before we tag.
- **Last shipped**: V2 web dashboard + tuning fixes merged to `main`.
- **Last paper run**: 2026-04-22 — 0 entries again. Hoping for a fill
  this week so the new dashboard (strategy stats, badges, ticker)
  has live data to show.
- **Build**: clean, zero warnings (`cargo build --workspace`).
- **Daemon**: stop button verified; live-mode hard block removed
  (gated only at startup via `live.enabled`).

## 🐛 Active bugs

_None open._

## V2.1 — wrap-up

Phase 1 + Phase 2 shipped. Phase 3 (Iron Condor) is deferred to **V2.3**
because the Alpaca multi-leg spreads application is still pending.
One feature still to land in V2.1: the weekly review report. We want
it driving the very first weekend tuning pass after the tag, so it
has to ship before the tag — not after.

- [x] Phase 1 — Strategy registry + attribution.
- [x] Phase 2 — Live toggle + UI plumbing + post-Phase-2 UX polish.

### Weekly strategy review report

Goal: a generated, readable digest at end of week (Friday after close)
that drives the weekend tuning pass. Replaces the current eyeball-scan
of logs. First run target: **Friday 2026-04-24** after close so we
have real output to read against before tagging.

- [ ] **Format**: markdown file emitted to `docs/reports/YYYY-MM-DD.md`,
      one per Friday. Same file overwrites if re-run that day.
- [ ] **Sections**:
  - Scan summary — total scans, by symbol, by regime, hit rate.
  - Veto breakdown — count by reason (extreme_proximity, sector cap,
    min_confluence, IV rank gate, etc.). The "why didn't we enter"
    column.
  - Near-miss table — symbols that scored within 1 point of
    `min_confluence_score` but were vetoed; what would have changed
    if the threshold were 1 lower.
  - Entries + exits — fills, P&L per trade, P&L per strategy,
    win rate, average hold, average winner / average loser.
  - Regime distribution — what % of scans saw TrendingUp /
    TrendingDown / RangeBound / Choppy.
  - One-line "verdict" the operator writes by hand at the top after
    reading, before tuning.
- [ ] **Implementation**: new `ferrum-report` CLI binary in the
      workspace (separate so it doesn't bloat the daemon), reads
      `~/.ferrum/ferrum.db` directly, takes `--week=YYYY-Www` (default
      = current ISO week), writes the markdown file.
- [ ] **First real run**: `cargo run -p ferrum-report` on Friday
      2026-04-24 after close. Read the output, sanity-check the
      numbers against the daemon logs, write a verdict by hand. If
      the report is broken or ugly, fix before tagging.
- [ ] **Schedule (deferred to V2.2)**: cron entry on the homelab once
      it's deployed — Friday 16:30 ET. Until then, manual invocation.

### Tag + branch out

- [ ] Tag `v2.1.0` once the weekly report has run cleanly at least
      once. Merge `V2.1` → `main`, annotated tag, push tag. Update
      `Last shipped` here once done.
- [ ] Spin off `V2.2` branch from the merged main right after the tag.

## V2.2 — homelab deployment + PDT rule change

Two big themes: get the bot deploying itself onto the homelab once the
hardware arrives, and rework the strategy now that the SEC PDT rule is
about to go away.

### Theme A — GitLab migration + CI/CD pipeline

We need GitLab as the CI host so the runner can deploy directly into
the homelab LAN (GitHub Actions can't reach a self-hosted target
without exposing a tunnel). The migration must be **non-disruptive** —
the current GitHub workflow keeps working until the cutover is verified.

- [ ] **Decide migration shape.** Options to research:
  1. **Push-mirror**: GitLab is the new primary; push-mirror back to
     GitHub for visibility. Cleanest long-term.
  2. **Pull-mirror**: GitHub stays primary; GitLab pulls and runs CI.
     Safest for "doesn't break our workflow".
  3. **Full migration**: archive GitHub. Most invasive.
  - Recommend starting with (2) for safety, then graduating to (1)
    once the pipeline has run green for a week.
- [ ] **GitLab project setup** — private repo, mirror configuration,
      protected branches matching current GitHub setup (`main` +
      version branches).
- [ ] **GitLab Runner** on the homelab (docker executor). Research
      whether the runner can be the same box as the deploy target or
      whether they should be separate for blast-radius reasons.
- [ ] **Pipeline stages** (`.gitlab-ci.yml`):
  - `lint`: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`,
    `npm run lint` in `web/`.
  - `test`: `cargo test --workspace` (currently no unit tests — note for
    future, don't block this on adding them).
  - `build`: `cargo build --release -p ferrum-daemon -p ferrum-web`,
    `npm run build` in `web/`. Cache Cargo registry + node_modules.
  - `package`: tarball with both binaries + `web/dist` + a
    `config.toml.example`. Tag triggers a "release" job that uploads
    the tarball to GitLab Releases.
  - `deploy`: only on tag (or `main` post-merge?), SSH into the homelab,
    rsync the tarball, restart the systemd unit. **Manual gate** for
    the first month — don't auto-deploy until we trust it.
- [ ] **Secrets**: Alpaca live + paper keys live in GitLab CI variables
      (masked, protected, file type for `config.toml` sections). The
      runner copies them into the deployed config at deploy time.
      Research: file vs. env-var injection, and whether sops/age would
      add value over just CI-managed secrets.
- [ ] **SSH deploy key**: dedicated key on the runner → restricted user
      on the homelab. Restricted user can only `systemctl restart
      ferrum-*` via sudoers, nothing else.
- [ ] **Rollback**: keep the previous tarball next to the active one.
      Deploy script symlinks `current → release-N`; rollback flips the
      symlink back. Document the manual command in the runbook.
- [ ] **Homelab box** — once the equipment arrives:
  - systemd units for `ferrum-daemon` and `ferrum-web` with
    `Restart=on-failure`, `WorkingDirectory=/opt/ferrum/current`.
  - Persistent data volume for `~/.ferrum/ferrum.db` (survives
    deploys).
  - LAN-only CORS / firewall rule so `ferrum-web` is only reachable
    from the LAN.
  - Log rotation for daemon stdout (or move to `journald` directly).
  - Cron entry `30 16 * * 5` running `ferrum-report` against the
    homelab DB so the Friday report fires unattended (manual until
    deploy lands — see V2.1 weekly-report section).

### Theme B — PDT rule removal + strategy rework

The SEC announced the PDT minimum drops to $2,000 and day trades
become unlimited above that. Alpaca devs estimated ~45 days from
announcement until the API reflects the change. We want the code ready
on day 1.

- [ ] **Confirm rule scope.** Research notes:
  - Does the new rule cover **options** too? PDT historically does, but
    the SEC release language needs to be re-read for the multi-leg
    spread case.
  - When does Alpaca's `account.pattern_day_trader` flag stop being
    enforced? Watch the Alpaca dev blog + API changelog.
  - Confirm the $2,000 floor is enforced by Alpaca (we're already above
    it on paper, but live needs the explicit check before we flip).
- [ ] **Add `[pdt] enabled = true` config flag.** Default `true` so
      nothing changes until the operator flips it on Alpaca-confirms-go
      day. All PDT logic gates on this flag — no code paths get
      removed, just bypassed when disabled. Reversible if the rule
      gets walked back.
- [ ] **Code paths to gate** (search: `PdtTracker|day_trade|same_day`):
  - `crates/ferrum-daemon/src/exit_monitor.rs` — PDT-aware close
    blocking + `force_exit_next_open` flag.
  - `crates/ferrum-daemon/src/order_poller.rs` — day-trade recording
    on close (keep recording for stats even when gating is off; just
    stop blocking on it).
  - `crates/ferrum-daemon/src/main.rs` — `PdtTracker::load_from_db`
    boot-time load (still useful for the dashboard counter; don't
    remove).
  - Web `Header.tsx` PDT counter — when disabled, replace with a
    grayed-out "PDT N/A" pill instead of removing (status indicator
    that the rule isn't blocking us anymore).
- [ ] **Strategy reset to take advantage of unlimited day trades.**
      Research first — pull a week of `scan_results` data and answer:
  - How many signals were vetoed by the PDT 1-slot reservation? (i.e.,
    we could have entered but didn't because we needed the slot for an
    emergency exit.)
  - How many same-day exits did we want to take but couldn't?
  - Which sectors clustered (auto, transport, comm) and would have
    benefited from a higher cap?
- [ ] **Sizing config rework** (post-research):
  - `sizing.max_open_positions`: currently 4. Likely 6–8 once the PDT
    reservation isn't needed. Tune based on the week-of-data.
  - `sizing.max_sector_positions`: currently 2. Revisit — purpose
    shifts from PDT-slot conservation to pure diversification, so it
    can probably stay at 2 but should be re-justified rather than
    inherited.
  - `sizing.max_portfolio_risk_pct`: re-derive given the higher
    position count.
- [ ] **Strategy doc update** — `docs/ferrum-forge-strategy.md` Section
      on PDT and sizing tables need a rewrite + a note that the
      pre-2026 rules are kept in the appendix for historical context.

### Backlog (still post-V2)
- Run Forge for a week with the 0.25 ATR veto and re-evaluate
  near-miss data (subsumed into the weekly report once that lands).
- Vertical credit spreads as a **fourth** strategy (after Iron Condor).
- Per-strategy P&L tiles in the dashboard.
- **StrategiesPanel — fixed footprint as the registry grows.** Today
  every strategy adds another row; with 3+ strategies the panel will
  dominate the right column. Convert to a single-strategy view with
  prev/next arrows (or a small dot pager / carousel). Keep the
  aggregate `N/M enabled` meta in the header so the operator can still
  see overall state at a glance. The toggle and per-strategy stats
  stay where they are inside the active card; arrows persist the
  selected strategy in `sessionStorage` so a reload doesn't reset to
  the first one.

## V2.3 — Iron Condor strategy

Pushed from V2.1 — Alpaca paper account is still under approval for
multi-leg spreads. Resume once the approval lands.

> Full design doc: [`docs/multi-strategy-plan.md`](docs/multi-strategy-plan.md)

**Manual prerequisite:** confirm multi-leg spread approval on Alpaca.

- [ ] Multi-leg `mleg` order support in `orders.rs`.
- [ ] `strategy/iron_condor.rs` — 4-strike selection by delta.
- [ ] `IronCondorEntryConfig` — `short_delta`, `wing_width_pct`,
      `min_credit_pct_of_width`.
- [ ] Condor sizing (max loss = wing × 100 − credit).
- [ ] Strategy-specific exits via `Strategy::check_exit`: 50% PT /
      2× credit stop / 21 DTE close.
- [ ] `OpenPositionMeta` evolves to optional multi-leg.
- [ ] Web `PositionsPanel` collapses 4 legs into one row.
- [ ] (Phase 1 carry-over) `Strategy::check_exit` trait method —
      deferred from V2.1 since only Forge needed exits at the time.
      Iron Condor forces the issue.

## To run

```bash
cargo run -p ferrum-daemon      # terminal 1 — leave running
cargo run -p ferrum-web         # terminal 2 — Axum server on :3000
cd web && npm run dev           # terminal 3 — Vite dev server, opens browser

# Production: ferrum-web serves the built React bundle directly
cd web && npm run build
cargo run -p ferrum-web         # dashboard on http://localhost:3000
```
