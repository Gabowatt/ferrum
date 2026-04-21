use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// Palette: #1b2021 → #51513d → #a6a867 → #e3dc95 → #e3dcc2
// Used as a top-lit gradient (cream = highlight, dark olive = shadow/base)
//
// Anvil icon — front silhouette with horn on left:
//
//   ▄████████    ← horn + flat top surface
//  ██████████    ← full body (widest)
//   ████████     ← slight taper
//     █████      ← narrow waist
//   ████████     ← stable base/feet

const ICON: &[&str] = &[
    "  ▄████████",
    " ███████████",
    "  █████████ ",
    "    █████   ",
    "  █████████ ",
];

const ICON_COLORS: &[Color] = &[
    Color::Rgb(227, 220, 194),  // #e3dcc2  cream   — top highlight
    Color::Rgb(227, 220, 149),  // #e3dc95  warm    — upper face
    Color::Rgb(166, 168, 103),  // #a6a867  olive   — mid body
    Color::Rgb( 81,  81,  61),  // #51513d  d-olive — waist shadow
    Color::Rgb( 81,  81,  61),  // #51513d  d-olive — base
];

/// Returns the 5 icon lines with top-lit gradient coloring.
pub fn logo_lines() -> Vec<Line<'static>> {
    ICON.iter().zip(ICON_COLORS.iter()).map(|(&row, &color)| {
        Line::from(Span::styled(
            row,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
    }).collect()
}

/// One-line tagline shown beneath the icon.
pub fn tagline() -> Line<'static> {
    Line::from(Span::styled(
        "  ferrum — quant options trader",
        Style::default().fg(Color::Rgb(81, 81, 61)),
    ))
}
