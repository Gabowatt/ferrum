# ferrum — Claude Code Build Plan

> Options trading bot + TUI in Rust. Work with Claude Code on a **commit-by-commit basis** so progress is never lost between sessions or daily limits.

---

## Ground rules for every session

- **Commit after every meaningful unit of work** — a compiling crate skeleton, a passing test, a wired-up endpoint. Never leave a session mid-feature without committing what compiles.
- Start each new session by running `git log --oneline -10` so Claude Code knows exactly where we left off.
- If a session ends mid-task, leave a `TODO.md` in the repo root describing the next immediate step.
- Live trading is **disabled** until explicitly unlocked. The `[alpaca.live]` block in `config.toml` exists but the daemon will refuse to use it until a feature flag is set.

---

## Phase 0 — Setup (do this first, one session)

### 0.1 GitHub
- [ ] Create repo `ferrum` on GitHub (private)
- [ ] `git init`, add `.gitignore` for Rust (`target/`, `*.env`, `config.toml`)
- [ ] Push initial commit with just `README.md` and `.gitignore`
- [ ] Confirm Claude Code can `git status`, `git add`, `git commit`, `git push`

### 0.2 Workspace scaffold
```
ferrum/
├── Cargo.toml              # workspace root
├── config.toml             # gitignored — local secrets + params
├── config.example.toml     # committed — safe template
├── TODO.md                 # current session checkpoint
├── crates/
│   ├── ferrum-daemon/      # core background service
│   ├── ferrum-tui/         # ratatui frontend
│   ├── ferrum-core/        # shared types, traits, errors
│   └── ferrum-export/      # tax/CSV export tooling
└── docs/
    └── build-plan.md       # this file
```

**Commit:** `chore: workspace scaffold`

### 0.3 API keys
`config.toml` (gitignored):
```toml
[alpaca]
mode = "paper"              # "paper" | "live" (live disabled in v1)

[alpaca.paper]
key    = "YOUR_PAPER_KEY"
secret = "YOUR_PAPER_SECRET"
base_url = "https://paper-api.alpaca.markets"

# Live block exists but daemon will panic if mode = "live" in v1
[alpaca.live]
key    = ""
secret = ""
base_url = "https://api.alpaca.markets"
enabled = false             # hard gate — daemon checks this on boot

[polygon]
key = "YOUR_POLYGON_KEY"

[risk]
max_position_usd  = 1000
daily_drawdown_pct = 2.0
max_open_legs      = 4

[strategy]
symbols = ["SPY", "QQQ", "AAPL"]
scan_interval_secs = 30
```

**Commit:** `chore: add config.example.toml`

---

## Phase 1 — Daemon foundation

> Goal: a running background process that connects to Alpaca paper, exposes IPC, and can be started/stopped cleanly.

### Milestone 1.1 — ferrum-core types
- `Mode` enum: `Paper` / `Live`
- `AppConfig` struct (deserialized from `config.toml` via `serde` + `toml`)
- `AlpacaClient` wrapper around `reqwest` with paper/live base URL switching
- `BotStatus` enum: `Idle` / `Running` / `Stopping`
- `LogEvent` struct: `{ timestamp, level, message }` — used by daemon → TUI log feed
- Error types via `thiserror`

**Crates:** `serde`, `toml`, `reqwest`, `thiserror`, `chrono`

**Commit:** `feat(core): config types, AlpacaClient stub, LogEvent`

### Milestone 1.2 — ferrum-daemon skeleton
- `main.rs` boots, reads config, validates live gate (`enabled = false` → refuse + log)
- Spawns tokio runtime
- Connects to Alpaca paper — GET `/v2/account` health check on startup
- Unix socket IPC server (path: `/tmp/ferrum.sock`) — accepts JSON commands:
  - `{ "cmd": "status" }` → returns `BotStatus` + mode
  - `{ "cmd": "start" }` → starts strategy loop
  - `{ "cmd": "stop" }` → graceful shutdown of strategy loop
  - `{ "cmd": "toggle_mode", "mode": "paper" }` → switches Alpaca client base URL (paper only in v1)
  - `{ "cmd": "get_pnl", "period": "1D" }` → proxies Alpaca portfolio history
  - `{ "cmd": "get_fills" }` → queries local SQLite
- Graceful `SIGINT`/`SIGTERM` handling via `tokio::signal`

**Crates:** `tokio`, `tokio::net::UnixListener`, `serde_json`, `signal-hook`

**Commit:** `feat(daemon): IPC server, Alpaca health check, graceful shutdown`

### Milestone 1.3 — SQLite persistence
- `ferrum-daemon` opens (or creates) `~/.local/share/ferrum/ferrum.db`
- Tables:
  - `fills` — mirrors Alpaca activity `FILL` events: `id, symbol, side, qty, price, timestamp, order_id`
  - `log_events` — persisted bot log: `id, timestamp, level, message`
  - `sessions` — `id, mode, started_at, stopped_at`
- On startup: replay any open session (crash recovery)
- Background task polls Alpaca `/v2/account/activities?activity_types=FILL` every 60s and upserts into `fills`

**Crates:** `sqlx` (SQLite, compile-time checked queries), `tokio`

**Commit:** `feat(daemon): SQLite schema, fill sync background task`

---

## Phase 2 — Strategy engine + Risk guard

### Milestone 2.1 — Strategy trait
```rust
#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    async fn scan(&self, client: &AlpacaClient) -> Result<Vec<Signal>>;
    async fn on_signal(&self, signal: Signal, client: &AlpacaClient) -> Result<Option<Order>>;
}

pub enum Signal {
    EnterLong  { symbol: String, legs: Vec<OptionLeg> },
    EnterShort { symbol: String, legs: Vec<OptionLeg> },
    Exit       { symbol: String },
}

pub struct OptionLeg {
    pub contract:   String,   // e.g. "AAPL260117C00200000"
    pub action:     LegAction, // Buy | Sell
    pub qty:        u32,
    pub order_type: OrderType, // Limit | Market
    pub limit_price: Option<f64>,
}
```

- Supports single-leg (calls/puts) and multi-leg (spreads, straddles) from the start
- Strategy engine runs scan loop on `scan_interval_secs` from config
- Emits `LogEvent` on every scan, signal, and order attempt

**Commit:** `feat(daemon): Strategy trait, Signal, OptionLeg types`

### Milestone 2.2 — Risk guard
Runs **before** every order submission — returns `Ok(())` or `Err(RiskViolation)`:
- Max position size in USD
- Daily drawdown % (reads from SQLite sessions table)
- Max open option legs at once
- Hard block if `mode == Live && config.alpaca.live.enabled == false`

**Commit:** `feat(daemon): RiskGuard, pre-order validation`

### Milestone 2.3 — First strategy: single-leg delta scan (stub)
- Polls Polygon for options chain on configured symbols
- Filters by delta range, IV percentile, DTE window (all configurable in `config.toml`)
- Emits `EnterLong` signals — no execution yet, log only
- Good for validating the scan loop + log feed before going live

**Commit:** `feat(daemon): DeltaScanStrategy stub, Polygon options chain fetch`

---

## Phase 3 — TUI (priority for V1)

> This is the primary interface. The web app comes later. Build this to be solid and informative.

### Milestone 3.1 — TUI skeleton + IPC client
- `ferrum-tui` binary connects to `/tmp/ferrum.sock` on launch
- If daemon not running: show "daemon offline" splash with instructions
- Main loop: `ratatui` event loop, polls IPC every 500ms for state updates
- Keybindings help bar at the bottom (always visible)

**Crates:** `ratatui`, `crossterm`, `tokio::net::UnixStream`

**Commit:** `feat(tui): skeleton, IPC client, daemon offline state`

### Milestone 3.2 — TUI layout

```
┌─────────────────────────────────────────────────────────────────┐
│ ferrum  [PAPER]  ● RUNNING          SPY QQQ AAPL     12:34:05  │
├───────────────────────┬─────────────────────────────────────────┤
│  Positions            │  PnL                                    │
│  AAPL 260117C200  +2  │  Today   +$142.30  (+1.4%)             │
│  SPY  260117P450  -1  │  Month   +$891.00  (+8.9%)             │
│                       │  Year    +$2,340   (+23%)              │
├───────────────────────┴─────────────────────────────────────────┤
│  Recent fills                                                   │
│  12:31  BUY  AAPL 260117C200  x2  @ $3.40                      │
│  11:55  SELL SPY  260117P450  x1  @ $2.10                      │
├─────────────────────────────────────────────────────────────────┤
│  Bot log                                                        │
│  12:34:05  [INFO]  Scanning SPY options chain...               │
│  12:34:03  [INFO]  No signal — IV rank below threshold (32%)   │
│  12:33:33  [INFO]  Scanning QQQ options chain...               │
│  12:33:31  [SIGNAL] AAPL delta 0.42 in range — EnterLong       │
│  12:31:02  [ORDER]  BUY AAPL 260117C200 x2 limit $3.40 → FILL  │
│  12:31:01  [RISK]   Guard passed — within limits               │
├─────────────────────────────────────────────────────────────────┤
│  [S] Start  [X] Stop  [M] Mode: Paper  [E] Export  [Q] Quit   │
└─────────────────────────────────────────────────────────────────┘
```

**Panels:**
- **Header bar** — mode badge (`PAPER` / `LIVE`), bot status dot (green=running, yellow=idle, red=stopped), active symbols, clock
- **Positions** — live open positions from Alpaca `/v2/positions`, refreshed every 10s
- **PnL** — today / month / year pulled from Alpaca portfolio history endpoint + local SQLite
- **Recent fills** — last 10 fills from local SQLite, newest first
- **Bot log** — scrollable, tail-follows by default, shows `INFO` / `SIGNAL` / `ORDER` / `RISK` / `ERROR` levels with color coding
- **Keybindings bar** — always visible at bottom

**Commit:** `feat(tui): full layout, positions, PnL, fills panels`

### Milestone 3.3 — Controls
- `[S]` — sends `{ "cmd": "start" }` over IPC → daemon starts strategy loop
- `[X]` — sends `{ "cmd": "stop" }` → graceful stop
- `[M]` — toggle paper/live (paper only in v1; shows warning modal if live attempted)
- `[E]` — triggers export menu (date range picker → CSV written to `~/ferrum-export-YYYY.csv`)
- `[Q]` — quits TUI (daemon keeps running in background)
- `[↑↓]` — scroll bot log
- `[?]` — toggle full keybindings help overlay

**Commit:** `feat(tui): controls, keybindings, paper/live toggle guard`

### Milestone 3.4 — Bot log feed
- Daemon pushes `LogEvent` structs over IPC (SSE-style: daemon writes newline-delimited JSON to socket)
- TUI reads stream in background tokio task, appends to in-memory ring buffer (last 500 events)
- Color by level: `INFO` = default, `SIGNAL` = amber, `ORDER` = teal, `RISK` = coral, `ERROR` = red
- Tail-follow toggles off when user scrolls up, re-engages on `[End]` or `[F]`

**Commit:** `feat(tui): live log stream, color levels, scroll + tail-follow`

---

## Phase 4 — Export + Tax tooling

### Milestone 4.1 — ferrum-export
- `get_fills --from YYYY-MM-DD --to YYYY-MM-DD` → queries SQLite
- Output formats: `--format csv` (default), `--format json`
- CSV columns: `date, symbol, side, qty, price, proceeds, cost_basis, gain_loss, holding_period`
- Holding period calculated from open→close fill pairs (short-term / long-term flag)
- TUI `[E]` export flow calls this internally

**Commit:** `feat(export): fill export, CSV/JSON output, holding period calc`

---

## Phase 5 — Web app (V2, future)

> Not part of V1. Documented here so the daemon IPC design accounts for it from the start.

- Add `axum` behind `--features web` compile flag
- Daemon spawns HTTP server on `config.web.port` (default 7878) when feature enabled
- REST endpoints mirror IPC commands: `GET /status`, `POST /start`, `POST /stop`, `GET /pnl`, `GET /fills`
- Web frontend (React or Leptos): remote config editor, PnL charts, fill history
- Auth: simple API key header (`X-Ferrum-Key`) configured in `config.toml`

---

## Dependency matrix

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime |
| `ratatui` + `crossterm` | TUI rendering |
| `reqwest` | HTTP — Alpaca + Polygon |
| `tokio-tungstenite` | WebSocket — live quote stream |
| `serde` + `toml` + `serde_json` | Config + IPC serialization |
| `sqlx` (SQLite) | Local fill + log persistence |
| `clap` | CLI args for export subcommands |
| `thiserror` | Error types |
| `chrono` | Timestamps + date math |
| `axum` (v2 only) | HTTP API for web app |
| `async-trait` | Strategy trait |

---

## Session checkpoint protocol

At the end of every Claude Code session:

1. Run `cargo check` — must pass before committing
2. `git add -A && git commit -m "<conventional commit message>"`
3. `git push origin main`
4. Update `TODO.md` with the next immediate step
5. Commit `TODO.md`: `chore: session checkpoint — next: <what's next>`

To resume: share `git log --oneline -10` and `cat TODO.md` at the start of the session.

---

## Conventional commit reference

```
feat(daemon):   new daemon capability
feat(tui):      new TUI panel or control
feat(core):     shared type or trait
feat(export):   export/tax tooling
fix(...):       bug fix
chore(...):     config, deps, scaffold
refactor(...):  no behavior change
test(...):      tests only
docs(...):      docs only
```
