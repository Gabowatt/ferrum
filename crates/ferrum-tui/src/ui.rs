use chrono::Local;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use ferrum_core::types::{BotStatus, LogEvent, LogLevel};
use crate::app::App;
use crate::logo;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn pdt_color(used: u32, max: u32) -> Color {
    if max == 0 { return Color::DarkGray; }
    let ratio = used as f32 / max as f32;
    if ratio >= 1.0      { Color::Red    }
    else if ratio >= 0.67 { Color::Yellow }
    else                  { Color::Green  }
}

pub fn draw(f: &mut Frame, app: &App) {
    if !app.daemon_online {
        draw_offline(f);
        return;
    }

    let area = f.area();

    // ── Top-level layout ──────────────────────────────────────────────────────
    // [header][positions+pnl][fills][log][keybindings]
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),  // header (logo 5 + tagline + status)
            Constraint::Length(6),  // positions + pnl
            Constraint::Length(5),  // recent fills
            Constraint::Min(6),     // bot log
            Constraint::Length(1),  // keybindings bar
        ])
        .split(area);

    draw_header(f, chunks[0], app);
    draw_positions_pnl(f, chunks[1], app);
    draw_fills(f, chunks[2], app);
    draw_log(f, chunks[3], app);
    draw_keybindings(f, chunks[4]);

    if app.show_help {
        draw_help_overlay(f, area);
    }
}

fn draw_offline(f: &mut Frame) {
    let area = f.area();
    let msg = vec![
        Line::from(Span::styled("ferrum daemon is offline", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("Start the daemon with:  cargo run -p ferrum-daemon"),
        Line::from(""),
        Line::from(Span::styled("[Q] Quit", Style::default().fg(Color::DarkGray))),
    ];
    let p = Paragraph::new(msg)
        .block(Block::default().borders(Borders::ALL).title(" ferrum "))
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    // Stack: 5 logo rows + tagline + status line
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // pixel art
            Constraint::Length(1),  // tagline
            Constraint::Length(1),  // status / pdt / clock
        ])
        .split(area);

    // Logo
    f.render_widget(Paragraph::new(logo::logo_lines()), rows[0]);

    // Tagline
    f.render_widget(Paragraph::new(logo::tagline()), rows[1]);

    // Status line
    let status_color = match app.bot_status {
        BotStatus::Running  => Color::Green,
        BotStatus::Idle     => Color::Yellow,
        BotStatus::Stopping => Color::Red,
    };
    let time = Local::now().format("%H:%M:%S").to_string();
    let pdt_col = pdt_color(app.pdt_used, app.pdt_max);
    let status_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("[{}]", app.mode), Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled("●", Style::default().fg(status_color)),
        Span::raw(format!(" {}  ", app.bot_status)),
        Span::raw("PDT: "),
        Span::styled(
            format!("{}/{}", app.pdt_used, app.pdt_max),
            Style::default().fg(pdt_col),
        ),
        Span::raw("  "),
        Span::styled(time, Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(status_line), rows[2]);
}

fn draw_positions_pnl(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // ── Positions panel ───────────────────────────────────────────────────────
    let pos_block = Block::default().borders(Borders::ALL).title(format!(
        " Positions ({}) ", app.positions.len()
    ));

    if app.positions.is_empty() {
        let pos_text = Paragraph::new("  (no open positions)")
            .block(pos_block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(pos_text, cols[0]);
    } else {
        let items: Vec<ListItem> = app.positions.iter().take(4).map(|pos| {
            let pnl_pct  = pos.unrealized_plpc * 100.0;
            let pnl_color = if pnl_pct >= 0.0 { Color::Green } else { Color::Red };
            let dir_color = if pos.direction == "call" { Color::Cyan } else { Color::Magenta };
            // Shorten contract: last 15 chars
            let contract_short = if pos.contract.len() > 18 {
                &pos.contract[pos.contract.len() - 18..]
            } else {
                &pos.contract
            };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<18}", contract_short),
                    Style::default().fg(dir_color),
                ),
                Span::raw(format!(" x{:.0}  @{:.2}  ", pos.qty, pos.entry_price)),
                Span::styled(
                    format!("{:+.1}%", pnl_pct),
                    Style::default().fg(pnl_color),
                ),
                Span::styled(
                    format!(" ({:+.0})", pos.unrealized_pl),
                    Style::default().fg(pnl_color),
                ),
            ]))
        }).collect();
        f.render_widget(List::new(items).block(pos_block), cols[0]);
    }

    // ── PnL panel ─────────────────────────────────────────────────────────────
    let pnl_color = |v: f64| if v >= 0.0 { Color::Green } else { Color::Red };
    let pnl_lines = vec![
        Line::from(vec![
            Span::raw("  Today   "),
            Span::styled(format!("{:+.2}", app.pnl_today), Style::default().fg(pnl_color(app.pnl_today))),
        ]),
        Line::from(vec![
            Span::raw("  Month   "),
            Span::styled(format!("{:+.2}", app.pnl_month), Style::default().fg(pnl_color(app.pnl_month))),
        ]),
        Line::from(vec![
            Span::raw("  Year    "),
            Span::styled(format!("{:+.2}", app.pnl_year), Style::default().fg(pnl_color(app.pnl_year))),
        ]),
    ];
    let pnl_block = Block::default().borders(Borders::ALL).title(" PnL ");
    f.render_widget(Paragraph::new(pnl_lines).block(pnl_block), cols[1]);
}

fn draw_fills(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.fills.iter().take(4).map(|fill| {
        let time = fill.timestamp.with_timezone(&Local).format("%H:%M").to_string();
        let side_color = if fill.side.to_lowercase() == "buy" { Color::Green } else { Color::Red };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {time}  "), Style::default().fg(Color::DarkGray)),
            Span::styled(fill.side.to_uppercase(), Style::default().fg(side_color)),
            Span::raw(format!("  {}  x{:.0}  @ ${:.2}", fill.symbol, fill.qty, fill.price)),
        ]))
    }).collect();

    let block = Block::default().borders(Borders::ALL).title(" Recent Fills ");
    f.render_widget(List::new(items).block(block), area);
}

fn draw_log(f: &mut Frame, area: Rect, app: &App) {
    let events: Vec<&LogEvent> = app.log_events.iter().collect();
    let scroll_offset = if app.tail_follow {
        events.len().saturating_sub(area.height as usize - 2)
    } else {
        app.log_scroll
    };

    let items: Vec<ListItem> = events
        .iter()
        .skip(scroll_offset)
        .map(|ev| {
            let time = ev.timestamp.with_timezone(&Local).format("%H:%M:%S").to_string();
            let (level_str, level_color) = match ev.level {
                LogLevel::Info   => ("INFO  ", Color::White),
                LogLevel::Signal => ("SIGNAL", Color::Yellow),
                LogLevel::Order  => ("ORDER ", Color::Cyan),
                LogLevel::Risk   => ("RISK  ", Color::LightRed),
                LogLevel::Error  => ("ERROR ", Color::Red),
                LogLevel::Warn   => ("WARN  ", Color::Magenta),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {time}  "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("[{level_str}]  "), Style::default().fg(level_color)),
                Span::raw(&ev.message),
            ]))
        })
        .collect();

    let title = if app.tail_follow {
        " Bot Log (following) "
    } else {
        " Bot Log (↑↓ scroll — [F]/[End] to follow) "
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(List::new(items).block(block), area);
}

fn draw_keybindings(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" [S]", Style::default().fg(Color::Green)),   Span::raw(" Start  "),
        Span::styled("[X]", Style::default().fg(Color::Red)),      Span::raw(" Stop  "),
        Span::styled("[M]", Style::default().fg(Color::Cyan)),     Span::raw(" Mode  "),
        Span::styled("[E]", Style::default().fg(Color::Yellow)),   Span::raw(" Export  "),
        Span::styled("[Q]", Style::default().fg(Color::DarkGray)), Span::raw(" Quit  "),
        Span::styled("[?]", Style::default().fg(Color::DarkGray)), Span::raw(" Help "),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 70, area);
    f.render_widget(Clear, popup);

    let text = vec![
        Line::from(Span::styled("Keybindings", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  [S]          Start strategy loop"),
        Line::from("  [X]          Stop strategy loop"),
        Line::from("  [M]          Toggle mode (paper/live)"),
        Line::from("  [E]          Export fills to CSV"),
        Line::from("  [Q]          Quit TUI (daemon keeps running)"),
        Line::from("  [↑] [↓]      Scroll bot log"),
        Line::from("  [End] [F]    Return to tail-follow mode"),
        Line::from("  [?]          Toggle this help overlay"),
        Line::from(""),
        Line::from(Span::styled("  Press [?] to close", Style::default().fg(Color::DarkGray))),
    ];
    f.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Help "))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(layout[1])[1]
}
