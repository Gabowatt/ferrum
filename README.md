# ferrum

Quant-level options trading bot + TUI, powered by Alpaca Trading.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  Phase 1                                                            │
│                                                                     │
│   ┌─────────────────┐      IPC (unix socket)     ┌──────────────┐  │
│   │   ferrum-tui    │ ◄──────────────────────── ►│              │  │
│   │  ratatui · TUI  │                            │              │  │
│   └─────────────────┘                            │              │  │
│                                                  │  ferrum-     │  │
│   ┌─────────────────┐                            │  daemon      │  │
│   │  config.toml    │ ──────────────────────────►│              │  │
│   │  keys · params  │                            │  ┌─────────┐ │  │
│   └─────────────────┘                            │  │Strategy │ │  │
│                                                  │  │ engine  │ │  │
│   ┌─────────────────┐                            │  ├─────────┤ │  │
│   │   local DB      │ ◄─────────────────────── ► │  │  State  │ │  │
│   │ SQLite · fills  │                            │  │ manager │ │  │
│   └─────────────────┘                            │  ├─────────┤ │  │
│                                                  │  │  Risk   │ │  │
│                                                  │  │  guard  │ │  │
│                                                  │  ├─────────┤ │  │
│                                                  │  │   IPC   │ │  │
│                                                  │  │ server  │ │  │
│                                                  └──┴────┬────┘ │  │
│                                                          │       │  │
│                              ┌───────────────┬───────────┘       │  │
│                              ▼               ▼                   │  │
│                     ┌──────────────┐ ┌──────────────┐           │  │
│                     │  Alpaca API  │ │ Polygon.io   │           │  │
│                     │ paper ↔ live │ │ options data │           │  │
│                     └──────────────┘ └──────────────┘           │  │
│                                                                     │
├─────────────────────────────────────────────────────────────────────┤
│  Phase 2 (V2 — future)                                             │
│                                                                     │
│   ┌─────────────────┐    REST (hosted anywhere)  ┌──────────────┐  │
│   │   Web app       │ ◄─────────────────────────►│  Axum HTTP   │  │
│   │ remote config   │                            │  API layer   │  │
│   └─────────────────┘                            └──────┬───────┘  │
│                                                         │           │
│                                                    connects to      │
│                                                    daemon IPC       │
└─────────────────────────────────────────────────────────────────────┘
```

**Key design decisions:**
- The daemon runs independently — TUI and (eventually) web app are just clients
- Closing the TUI does **not** stop the bot
- Paper/live switch happens inside the daemon; clients just send the toggle command
- All external calls (Alpaca + Polygon) go through the daemon only — never from the TUI directly

## Workspace structure

```
ferrum/
├── Cargo.toml              # workspace root
├── config.toml             # gitignored — local secrets + params
├── config.example.toml     # committed — safe template
├── crates/
│   ├── ferrum-core/        # shared types, traits, errors
│   ├── ferrum-daemon/      # core background service
│   ├── ferrum-tui/         # ratatui frontend
│   └── ferrum-export/      # tax/CSV export tooling
└── ferrum-build-plan.md    # full phase-by-phase build plan
```

The build plan (`ferrum-build-plan.md`) is the authoritative reference for every milestone, commit convention, dependency choice, and session checkpoint protocol. Each Claude Code session starts by reading it alongside `git log --oneline -10` and `TODO.md`.

## Quickstart

1. Copy `config.example.toml` to `config.toml` and fill in your Alpaca paper keys.
2. Run the daemon in one terminal:
   ```
   cargo run -p ferrum-daemon
   ```
3. Run the TUI in a second terminal:
   ```
   cargo run -p ferrum-tui
   ```

The daemon runs in the background — closing the TUI does not stop it. Send `SIGINT` (`Ctrl-C`) to the daemon process to shut it down cleanly.

## Safety

Live trading is **disabled** in V1. The daemon will refuse to start in live mode regardless of config. See `[alpaca.live]` in `config.example.toml`.
