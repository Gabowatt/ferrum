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

// ── Tokyo Night palette ───────────────────────────────────────────────────────
const DIM:    Color = Color::Rgb( 65,  72, 104);  // #414868  borders / subtle
const MID:    Color = Color::Rgb( 86,  95, 137);  // #565f89  secondary text
const FG:     Color = Color::Rgb(169, 177, 214);  // #a9b1d6  normal text
const BRIGHT: Color = Color::Rgb(192, 202, 245);  // #c0caf5  bright text
const BLUE:   Color = Color::Rgb(122, 162, 247);  // #7aa2f7  highlight / info
const CYAN:   Color = Color::Rgb(125, 207, 255);  // #7dcfff  secondary highlight
const GREEN:  Color = Color::Rgb(158, 206, 106);  // #9ece6a  profit / up
const YELLOW: Color = Color::Rgb(224, 175, 104);  // #e0af68  warn
const ORANGE: Color = Color::Rgb(255, 158, 100);  // #ff9e64  order
const RED:    Color = Color::Rgb(247, 118, 142);  // #f7768e  loss / error
const PURPLE: Color = Color::Rgb(187, 154, 247);  // #bb9af7  signal

// ── Style helpers ─────────────────────────────────────────────────────────────

fn dim()    -> Style { Style::default().fg(DIM)    }
fn normal() -> Style { Style::default().fg(FG)     }
fn bright() -> Style { Style::default().fg(BRIGHT) }
fn head()   -> Style { Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD) }

fn bordered(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(dim())
        .title(Span::styled(format!(" {title} "), Style::default().fg(BLUE)))
}

fn pnl_color(v: f64) -> Color { if v >= 0.0 { GREEN } else { RED } }

fn pdt_color(used: u32, max: u32) -> Color {
    if max == 0 { return DIM; }
    let r = used as f32 / max as f32;
    if r >= 1.0       { RED    }
    else if r >= 0.67 { YELLOW }
    else              { GREEN  }
}

// ── Top-level draw ────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &App) {
    if !app.daemon_online {
        draw_offline(f, app);
        return;
    }

    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),  // header
            Constraint::Length(6),  // positions + pnl
            Constraint::Length(5),  // recent fills
            Constraint::Min(6),     // bot log
            Constraint::Length(1),  // keybindings
        ])
        .split(area);

    draw_header(f, chunks[0], app);
    draw_positions_pnl(f, chunks[1], app);
    draw_fills(f, chunks[2], app);
    draw_log(f, chunks[3], app);
    draw_keybindings(f, chunks[4], app);

    if app.show_help {
        draw_help_overlay(f, area);
    }
}

// ── Offline splash ────────────────────────────────────────────────────────────

fn draw_offline(f: &mut Frame, app: &App) {
    let area = f.area();
    let hint = if app.daemon_managed {
        "  starting daemon…"
    } else {
        "  cargo run -p ferrum-daemon   or press [D] to launch"
    };
    let msg = vec![
        Line::from(Span::styled(
            "ferrum daemon is offline",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(hint, normal())),
        Line::from(""),
        Line::from(Span::styled("[D] Launch daemon  [Q] Quit", dim())),
    ];
    f.render_widget(
        Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).border_style(dim())
                .title(Span::styled(" ferrum ", head())))
            .wrap(Wrap { trim: false }),
        area,
    );
}

// ── Header ────────────────────────────────────────────────────────────────────

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    f.render_widget(Paragraph::new(logo::logo_lines()), rows[0]);
    f.render_widget(Paragraph::new(logo::tagline()), rows[1]);

    let bot_color = match app.bot_status {
        BotStatus::Running  => GREEN,
        BotStatus::Idle     => MID,
        BotStatus::Stopping => RED,
    };

    let (market_dot, market_label, market_color) = match app.market_open {
        Some(true)  => ("●", "OPEN",   GREEN),
        Some(false) => ("●", "CLOSED", MID),
        None        => ("○", "?",      MID),
    };

    let next = if app.market_next_change.is_empty() {
        String::new()
    } else {
        format!(" ({})", app.market_next_change)
    };

    let time = Local::now().format("%H:%M:%S").to_string();
    let pdt_col = pdt_color(app.pdt_used, app.pdt_max);

    let mut status_spans = vec![
        Span::raw("  "),
        Span::styled(format!("[{}]", app.mode), Style::default().fg(CYAN)),
        Span::raw("  "),
        Span::styled("●", Style::default().fg(bot_color)),
        Span::styled(format!(" {}  ", app.bot_status), Style::default().fg(bot_color)),
        Span::styled("PDT ", dim()),
        Span::styled(format!("{}/{}", app.pdt_used, app.pdt_max), Style::default().fg(pdt_col)),
        Span::raw("  "),
        Span::styled(market_dot, Style::default().fg(market_color)),
        Span::raw(" "),
        Span::styled(market_label, Style::default().fg(market_color)),
        Span::styled(next, dim()),
        Span::raw("  "),
        Span::styled(time, dim()),
    ];
    if app.daemon_managed {
        status_spans.push(Span::raw("  "));
        status_spans.push(Span::styled("[managed]", Style::default().fg(PURPLE)));
    }

    f.render_widget(Paragraph::new(Line::from(status_spans)), rows[2]);
}

// ── Positions + PnL ───────────────────────────────────────────────────────────

fn draw_positions_pnl(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let pos_block = bordered("Positions").title_bottom(
        Span::styled(format!(" {} open ", app.positions.len()), dim()),
    );

    if app.positions.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("  no open positions", dim())).block(pos_block),
            cols[0],
        );
    } else {
        let items: Vec<ListItem> = app.positions.iter().take(4).map(|pos| {
            let pnl_pct   = pos.unrealized_plpc * 100.0;
            let dir_color = if pos.direction == "call" { CYAN } else { BLUE };
            let contract_short = if pos.contract.len() > 18 {
                &pos.contract[pos.contract.len() - 18..]
            } else {
                &pos.contract
            };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<18}", contract_short), Style::default().fg(dir_color)),
                Span::styled(format!(" x{:.0}  @{:.2}  ", pos.qty, pos.entry_price), normal()),
                Span::styled(format!("{:+.1}%", pnl_pct), Style::default().fg(pnl_color(pnl_pct))),
                Span::styled(format!(" ({:+.0})", pos.unrealized_pl), Style::default().fg(pnl_color(pnl_pct))),
            ]))
        }).collect();
        f.render_widget(List::new(items).block(pos_block), cols[0]);
    }

    let pnl_lines = vec![
        Line::from(vec![
            Span::styled("  Today  ", dim()),
            Span::styled(format!("{:+.2}", app.pnl_today), Style::default().fg(pnl_color(app.pnl_today))),
        ]),
        Line::from(vec![
            Span::styled("  Month  ", dim()),
            Span::styled(format!("{:+.2}", app.pnl_month), Style::default().fg(pnl_color(app.pnl_month))),
        ]),
        Line::from(vec![
            Span::styled("  Year   ", dim()),
            Span::styled(format!("{:+.2}", app.pnl_year), Style::default().fg(pnl_color(app.pnl_year))),
        ]),
    ];
    f.render_widget(Paragraph::new(pnl_lines).block(bordered("PnL")), cols[1]);
}

// ── Recent fills ──────────────────────────────────────────────────────────────

fn draw_fills(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.fills.iter().take(4).map(|fill| {
        let time       = fill.timestamp.with_timezone(&Local).format("%H:%M").to_string();
        let side_color = if fill.side.to_lowercase() == "buy" { GREEN } else { RED };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {time}  "), dim()),
            Span::styled(fill.side.to_uppercase(), Style::default().fg(side_color).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}  x{:.0}  @ ${:.2}", fill.symbol, fill.qty, fill.price), normal()),
        ]))
    }).collect();
    f.render_widget(List::new(items).block(bordered("Recent Fills")), area);
}

// ── Bot log ───────────────────────────────────────────────────────────────────

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
                LogLevel::Info   => ("INFO  ", BLUE),
                LogLevel::Signal => ("SIGNAL", CYAN),
                LogLevel::Order  => ("ORDER ", ORANGE),
                LogLevel::Risk   => ("RISK  ", YELLOW),
                LogLevel::Error  => ("ERROR ", RED),
                LogLevel::Warn   => ("WARN  ", YELLOW),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {time}  "), dim()),
                Span::styled(format!("[{level_str}]  "), Style::default().fg(level_color)),
                Span::styled(ev.message.clone(), normal()),
            ]))
        })
        .collect();

    let title = if app.tail_follow {
        "Bot Log ↓"
    } else {
        "Bot Log  ↑↓ scroll · [F]/[End] follow"
    };

    f.render_widget(List::new(items).block(bordered(title)), area);
}

// ── Keybindings bar ───────────────────────────────────────────────────────────

fn draw_keybindings(f: &mut Frame, area: Rect, app: &App) {
    let d_label = if app.daemon_managed { " Kill daemon  " } else { " Launch daemon  " };
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("[S]", Style::default().fg(GREEN)),  Span::styled(" Start  ", dim()),
        Span::styled("[X]", Style::default().fg(RED)),    Span::styled(" Stop  ", dim()),
        Span::styled("[D]", Style::default().fg(PURPLE)), Span::styled(d_label, dim()),
        Span::styled("[E]", Style::default().fg(YELLOW)), Span::styled(" Export  ", dim()),
        Span::styled("[Q]", dim()),                       Span::styled(" Quit  ", dim()),
        Span::styled("[?]", dim()),                       Span::styled(" Help", dim()),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

// ── Help overlay ──────────────────────────────────────────────────────────────

fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 70, area);
    f.render_widget(Clear, popup);

    let text = vec![
        Line::from(Span::styled("Keybindings", head())),
        Line::from(""),
        Line::from(Span::styled("  [S]          Start strategy loop",              Style::default().fg(FG))),
        Line::from(Span::styled("  [X]          Stop strategy loop",               Style::default().fg(FG))),
        Line::from(Span::styled("  [D]          Launch / kill daemon process",     Style::default().fg(FG))),
        Line::from(Span::styled("  [E]          Export fills to CSV",              Style::default().fg(FG))),
        Line::from(Span::styled("  [Q]          Quit TUI (daemon keeps running)",  Style::default().fg(FG))),
        Line::from(Span::styled("  [↑] [↓]      Scroll bot log",                  Style::default().fg(FG))),
        Line::from(Span::styled("  [End] [F]    Return to tail-follow",            Style::default().fg(FG))),
        Line::from(Span::styled("  [?]          Toggle this overlay",              Style::default().fg(FG))),
        Line::from(""),
        Line::from(Span::styled("  Press [?] to close", dim())),
    ];
    f.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).border_style(dim())
                .title(Span::styled(" Help ", Style::default().fg(BLUE))))
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
