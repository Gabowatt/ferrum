mod app;
mod ipc;
mod logo;
mod ui;

use std::time::Duration;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use ferrum_core::types::IpcResponse;
use app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }
    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> anyhow::Result<()> {
    let mut app = App::new();
    let mut ipc = ipc::IpcClient::connect().await;
    if ipc.is_some() {
        app.daemon_online = true;
    }

    let mut last_clock_poll = std::time::Instant::now()
        - std::time::Duration::from_secs(61);
    let mut last_log_poll = std::time::Instant::now()
        - std::time::Duration::from_secs(3);

    loop {
        if let Some(ref mut client) = ipc {
            match client.request_status().await {
                Ok(IpcResponse::Status { status, mode }) => {
                    app.daemon_online = true;
                    app.bot_status    = status;
                    app.mode          = mode;
                }
                _ => {
                    app.daemon_online = false;
                    ipc = None;
                }
            }

            if let Some(ref mut client) = ipc {
                if let Ok(IpcResponse::Fills { fills }) = client.request_fills().await {
                    app.fills = fills;
                }
                if let Ok(IpcResponse::Pnl { today, month, year }) = client.request_pnl("1M").await {
                    app.pnl_today = today;
                    app.pnl_month = month;
                    app.pnl_year  = year;
                }
                if let Ok(positions) = client.request_positions().await {
                    app.positions = positions;
                }
                if let Ok((used, max)) = client.request_pdt().await {
                    app.pdt_used = used;
                    app.pdt_max  = max;
                }
                if last_clock_poll.elapsed() >= Duration::from_secs(60) {
                    if let Ok((is_open, next_change)) = client.request_market_clock().await {
                        app.market_open        = Some(is_open);
                        app.market_next_change = next_change;
                    }
                    last_clock_poll = std::time::Instant::now();
                }

                // Poll log events from DB every 2s.
                if last_log_poll.elapsed() >= Duration::from_secs(2) {
                    if let Ok(events) = client.request_logs(200).await {
                        for ev in events {
                            let is_new = match app.last_log_ts {
                                Some(last) => ev.timestamp > last,
                                None       => true,
                            };
                            if is_new {
                                app.last_log_ts = Some(ev.timestamp);
                                app.push_log(ev);
                            }
                        }
                    }
                    last_log_poll = std::time::Instant::now();
                }
            }
        } else {
            // Retry IPC connection.
            ipc = ipc::IpcClient::connect().await;
            if ipc.is_some() {
                app.daemon_online = true;
            }
        }

        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) | (KeyCode::Char('Q'), _) => break,
                    (KeyCode::Char('c'), KeyModifiers::CONTROL)        => break,

                    (KeyCode::Char('s'), _) | (KeyCode::Char('S'), _) => {
                        if let Some(ref mut client) = ipc {
                            let _ = client.send_start().await;
                        }
                    }
                    (KeyCode::Char('x'), _) | (KeyCode::Char('X'), _) => {
                        if let Some(ref mut client) = ipc {
                            let _ = client.send_stop().await;
                        }
                    }
                    (KeyCode::Char('?'), _) => {
                        app.show_help = !app.show_help;
                    }
                    (KeyCode::Char('h'), _) | (KeyCode::Char('H'), _) => {
                        app.pnl_hidden = !app.pnl_hidden;
                    }
                    (KeyCode::Up, _) => {
                        app.log_scroll  = app.log_scroll.saturating_sub(1);
                        app.tail_follow = false;
                    }
                    (KeyCode::Down, _) => {
                        app.log_scroll += 1;
                        app.tail_follow = false;
                    }
                    (KeyCode::End, _) | (KeyCode::Char('f'), _) => {
                        app.tail_follow = true;
                    }
                    _ => {}
                }
            }
        }

        // Drain any pushed log events from the IPC stream.
        if let Some(ref mut client) = ipc {
            while let Some(event) = client.poll_log_event() {
                app.push_log(event);
            }
        }
    }

    Ok(())
}
