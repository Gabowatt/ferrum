mod app;
mod ipc;
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
    // Set up terminal.
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal).await;

    // Restore terminal.
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

    // Attempt IPC connection.
    let mut ipc = ipc::IpcClient::connect().await;
    if ipc.is_none() {
        app.daemon_online = false;
    }

    loop {
        // Poll daemon for state updates every 500ms.
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
            }
        } else {
            // Retry connection.
            ipc = ipc::IpcClient::connect().await;
        }

        terminal.draw(|f| ui::draw(f, &app))?;

        // Poll for input with 500ms timeout.
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
                    (KeyCode::Up, _) => {
                        app.log_scroll = app.log_scroll.saturating_sub(1);
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

        // Drain log events from IPC stream.
        if let Some(ref mut client) = ipc {
            while let Some(event) = client.poll_log_event() {
                app.push_log(event);
            }
        }
    }

    Ok(())
}
