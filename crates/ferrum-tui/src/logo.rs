use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// Pixel art for "FERRUM" — 5 rows, single-block pixels
// Each letter is hand-designed on a 4-wide grid with 1-space gap
//
//  F     E     R     R     U     M
//  ████  ████  ███   ███   █  █  █   █
//  █     █     █  █  █  █  █  █  ██ ██
//  ███   ███   ███   ███   █  █  █ █ █
//  █     █     █ █   █ █   █  █  █   █
//  █     ████  █  █  █  █  ████  █   █

const ROWS: &[&str] = &[
    "████  ████  ███   ███   █  █  █   █",
    "█     █     █  █  █  █  █  █  ██ ██",
    "███   ███   ███   ███   █  █  █ █ █",
    "█     █     █ █   █ █   █  █  █   █",
    "█     ████  █  █  █  █  ████  █   █",
];

// Gradient: top rows are bright amber, bottom rows shift toward orange-red
// giving a "glowing hot iron" effect
const ROW_COLORS: &[Color] = &[
    Color::Rgb(255, 200,  60),  // bright gold
    Color::Rgb(255, 165,  30),  // amber
    Color::Rgb(255, 130,  20),  // orange
    Color::Rgb(220,  90,  10),  // deep orange
    Color::Rgb(180,  50,   5),  // red-orange (cooling iron)
];

/// Returns the 5 logo lines with gradient coloring, padded to `width`.
pub fn logo_lines() -> Vec<Line<'static>> {
    ROWS.iter().zip(ROW_COLORS.iter()).map(|(&row, &color)| {
        Line::from(Span::styled(
            format!("  {row}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
    }).collect()
}

/// One-line tagline shown beneath the logo.
pub fn tagline() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "  ⚒  quant options trader",
            Style::default().fg(Color::Rgb(120, 120, 120)),
        ),
    ])
}
