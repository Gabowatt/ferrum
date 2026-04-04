# ferrum

Quant-level options trading bot + TUI, powered by Alpaca Trading.

## Structure

```
ferrum/
├── Cargo.toml              # workspace root
├── config.toml             # gitignored — local secrets + params
├── config.example.toml     # committed — safe template
├── crates/
│   ├── ferrum-daemon/      # core background service
│   ├── ferrum-tui/         # ratatui frontend
│   ├── ferrum-core/        # shared types, traits, errors
│   └── ferrum-export/      # tax/CSV export tooling
└── docs/
    └── build-plan.md
```

## Quickstart

1. Copy `config.example.toml` to `config.toml` and fill in your Alpaca paper keys.
2. Run the daemon: `cargo run -p ferrum-daemon`
3. Run the TUI: `cargo run -p ferrum-tui`

## Safety

Live trading is **disabled** in V1. The daemon will refuse to start in live mode regardless of config.
