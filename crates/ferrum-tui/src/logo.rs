use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// Tokyo Night gradient: dark → blue → cyan → bright
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
    Color::Rgb(125, 207, 255),  // #7dcfff  cyan    — top highlight
    Color::Rgb(122, 162, 247),  // #7aa2f7  blue    — upper face
    Color::Rgb(122, 162, 247),  // #7aa2f7  blue    — mid body
    Color::Rgb( 65,  72, 104),  // #414868  dim     — waist shadow
    Color::Rgb( 86,  95, 137),  // #565f89  mid     — base
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
        Style::default().fg(Color::Rgb(86, 95, 137)),
    ))
}
