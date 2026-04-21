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

// ── Palette ───────────────────────────────────────────────────────────────────
const OLIVE:  Color = Color::Rgb( 81,  81,  61);  // #51513d  dark olive
const MID:    Color = Color::Rgb(166, 168, 103);  // #a6a867  olive
const WARM:   Color = Color::Rgb(227, 220, 149);  // #e3dc95  warm yellow
const CREAM:  Color = Color::Rgb(227, 220, 194);  // #e3dcc2  cream

// Keep traffic-light semantics for P&L / buy-sell — palette can't replace those.
const UP:    Color = Color::Rgb(130, 190, 100);   // muted green  (profit)
const DOWN:  Color = Color::Rgb(200,  80,  80);   // muted red    (loss)

// ── Style helpers ─────────────────────────────────────────────────────────────

fn dim()    -> Style { Style::default().fg(OLIVE) }
fn normal() -> Style { Style::default().fg(MID)   }
fn bright() -> Style { Style::default().fg(WARM)  }
fn head()   -> Style { Style::default().fg(CREAM).add_modifier(Modifier::BOLD) }

fn bordered(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(dim())
        .title(Span::styled(format!(" {title} "), bright()))
}

fn pnl_color(v: f64) -> Color { if v >= 0.0 { UP } else { DOWN } }

fn pdt_color(used: u32, max: u32) -> Color {
    if max == 0 { return OLIVE; }
    let r = used as f32 / max as f32;
    if r >= 1.0       { DOWN }
    else if r >= 0.67 { WARM }
    else              { UP   }
}

// ── Top-level draw ────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &App) {
    if !app.daemon_online {
        draw_offline(f);
        return;
    }

    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),  // header: icon(5) + tagline + status
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
    draw_keybindings(f, chunks[4]);

    if app.show_help {
        draw_help_overlay(f, area);
    }
}

// ── Offline splash ────────────────────────────────────────────────────────────

fn draw_offline(f: &mut Frame) {
    let area = f.area();
    let msg = vec![
        Line::from(Span::styled(
            "ferrum daemon is offline",
            Style::default().fg(DOWN).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  cargo run -p ferrum-daemon",
            normal(),
        )),
        Line::from(""),
        Line::from(Span::styled("[Q] Quit", dim())),
    ];
    f.render_widget(
        Paragraph::new(msg)
            .block(Block::default().borders(Borders::ALL).border_style(dim()).title(
                Span::styled(" ferrum ", head()),
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

// ── Header ────────────────────────────────────────────────────────────────────

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // anvil icon
            Constraint::Length(1),  // tagline
            Constraint::Length(1),  // status line
        ])
        .split(area);

    f.render_widget(Paragraph::new(logo::logo_lines()), rows[0]);
    f.render_widget(Paragraph::new(logo::tagline()), rows[1]);

    // ── Status line ───────────────────────────────────────────────────────────
    let bot_color = match app.bot_status {
        BotStatus::Running  => UP,
        BotStatus::Idle     => OLIVE,
        BotStatus::Stopping => DOWN,
    };

    let (market_dot, market_label, market_color) = match app.market_open {
        Some(true)  => ("●", "OPEN",    UP),
        Some(false) => ("●", "CLOSED",  OLIVE),
        None        => ("○", "?",       OLIVE),
    };

    let next = if app.market_next_change.is_empty() {
        String::new()
    } else {
        format!(" ({})", app.market_next_change)
    };

    let time = Local::now().format("%H:%M:%S").to_string();
    let pdt_col = pdt_color(app.pdt_used, app.pdt_max);

    let status_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("[{}]", app.mode), Style::default().fg(MID)),
        Span::raw("  "),
        Span::styled("●", Style::default().fg(bot_color)),
        Span::styled(format!(" {}  ", app.bot_status), Style::default().fg(bot_color)),
        Span::styled("PDT ", dim()),
        Span::styled(
            format!("{}/{}", app.pdt_used, app.pdt_max),
            Style::default().fg(pdt_col),
        ),
        Span::raw("  "),
        Span::styled(market_dot, Style::default().fg(market_color)),
        Span::raw(" "),
        Span::styled(market_label, Style::default().fg(market_color)),
        Span::styled(next, dim()),
        Span::raw("  "),
        Span::styled(time, dim()),
    ]);
    f.render_widget(Paragraph::new(status_line), rows[2]);
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
            let dir_color = if pos.direction == "call" { MID } else { WARM };
            let contract_short = if pos.contract.len() > 18 {
                &pos.contract[pos.contract.len() - 18..]
            } else {
                &pos.contract
            };
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<18}", contract_short), Style::default().fg(dir_color)),
                Span::styled(
                    format!(" x{:.0}  @{:.2}  ", pos.qty, pos.entry_price),
                    normal(),
                ),
                Span::styled(
                    format!("{:+.1}%", pnl_pct),
                    Style::default().fg(pnl_color(pnl_pct)),
                ),
                Span::styled(
                    format!(" ({:+.0})", pos.unrealized_pl),
                    Style::default().fg(pnl_color(pnl_pct)),
                ),
            ]))
        }).collect();
        f.render_widget(List::new(items).block(pos_block), cols[0]);
    }

    let pnl_lines = vec![
        Line::from(vec![
            Span::styled("  Today  ", dim()),
            Span::styled(
                format!("{:+.2}", app.pnl_today),
                Style::default().fg(pnl_color(app.pnl_today)),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Month  ", dim()),
            Span::styled(
                format!("{:+.2}", app.pnl_month),
                Style::default().fg(pnl_color(app.pnl_month)),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Year   ", dim()),
            Span::styled(
                format!("{:+.2}", app.pnl_year),
                Style::default().fg(pnl_color(app.pnl_year)),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(pnl_lines).block(bordered("PnL")), cols[1]);
}

// ── Recent fills ──────────────────────────────────────────────────────────────

fn draw_fills(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.fills.iter().take(4).map(|fill| {
        let time       = fill.timestamp.with_timezone(&Local).format("%H:%M").to_string();
        let side_color = if fill.side.to_lowercase() == "buy" { UP } else { DOWN };
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {time}  "), dim()),
            Span::styled(fill.side.to_uppercase(), Style::default().fg(side_color).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("  {}  x{:.0}  @ ${:.2}", fill.symbol, fill.qty, fill.price),
                normal(),
            ),
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
                LogLevel::Info   => ("INFO  ", MID),
                LogLevel::Signal => ("SIGNAL", WARM),
                LogLevel::Order  => ("ORDER ", CREAM),
                LogLevel::Risk   => ("RISK  ", DOWN),
                LogLevel::Error  => ("ERROR ", DOWN),
                LogLevel::Warn   => ("WARN  ", WARM),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {time}  "), dim()),
                Span::styled(format!("[{level_str}]  "), Style::default().fg(level_color)),
                Span::styled(&ev.message, normal()),
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

fn draw_keybindings(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("[S]", Style::default().fg(UP)),   Span::styled(" Start  ", dim()),
        Span::styled("[X]", Style::default().fg(DOWN)),  Span::styled(" Stop  ", dim()),
        Span::styled("[M]", Style::default().fg(MID)),   Span::styled(" Mode  ", dim()),
        Span::styled("[E]", Style::default().fg(WARM)),  Span::styled(" Export  ", dim()),
        Span::styled("[Q]", dim()),                      Span::styled(" Quit  ", dim()),
        Span::styled("[?]", dim()),                      Span::styled(" Help", dim()),
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
        Line::from(Span::styled("  [S]          Start strategy loop",   normal())),
        Line::from(Span::styled("  [X]          Stop strategy loop",    normal())),
        Line::from(Span::styled("  [M]          Toggle mode",           normal())),
        Line::from(Span::styled("  [E]          Export fills to CSV",   normal())),
        Line::from(Span::styled("  [Q]          Quit TUI (daemon keeps running)", normal())),
        Line::from(Span::styled("  [↑] [↓]      Scroll bot log",        normal())),
        Line::from(Span::styled("  [End] [F]    Return to tail-follow", normal())),
        Line::from(Span::styled("  [?]          Toggle this overlay",   normal())),
        Line::from(""),
        Line::from(Span::styled("  Press [?] to close", dim())),
    ];
    f.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).border_style(dim())
                .title(Span::styled(" Help ", bright())))
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
