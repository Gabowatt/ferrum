# ferrum — session checkpoint

## Completed this session

- [x] Phase 0: workspace scaffold (Cargo.toml, config.example.toml, README, .gitignore)
- [x] Milestone 1.1: ferrum-core — AppConfig, Mode, AlpacaClient, BotStatus, LogEvent, IPC types, Signal/OptionLeg
- [x] Milestone 1.2: ferrum-daemon — main boot, live gate, Alpaca health check, Unix socket IPC server, SIGINT/SIGTERM shutdown
- [x] Milestone 1.3: ferrum-daemon — SQLite schema (fills, log_events, sessions), fill sync background task (60s poll)
- [x] Milestone 2.1: Strategy trait, scan loop wired to DeltaScanStrategy stub
- [x] Milestone 2.2: RiskGuard — max position USD, daily drawdown, max open legs, live trading hard block
- [x] Milestone 2.3: DeltaScanStrategy stub (Polygon fetch not yet wired)
- [x] Milestone 3.1–3.4: ferrum-tui — full ratatui layout, IPC client, offline splash, keybindings, help overlay, log ring buffer, tail-follow scroll
- [x] ferrum-export stub binary

## Next immediate step

### Wire up Polygon options chain fetch in DeltaScanStrategy (Milestone 2.3 completion)

1. Add `[polygon]` HTTP calls to `strategy.rs` — GET `https://api.polygon.io/v3/snapshot/options/{symbol}` with delta/DTE filtering
2. Parse response into `OptionLeg` + emit real `EnterLong` signals
3. Confirm log events flow from daemon → TUI log panel

### After that

- Add live positions panel: GET `/v2/positions` in daemon, send via IPC `get_positions` command
- Wire PnL month/year from Alpaca portfolio history
- Test end-to-end: daemon running, TUI connected, [S] starts scan, log events appear in TUI

## To run locally

```bash
cp config.example.toml config.toml
# fill in your Alpaca paper API key/secret

cargo run -p ferrum-daemon   # terminal 1
cargo run -p ferrum-tui      # terminal 2
```
