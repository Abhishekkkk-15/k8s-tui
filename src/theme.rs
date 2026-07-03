//! Minimal monochrome palette. Color is spent only on things that matter:
//! resource status severity and the current selection/focus.

use ratatui::style::{Color, Modifier, Style};

use crate::data::Severity;

pub const BG: Color = Color::Rgb(12, 12, 14);
pub const BG_PANEL: Color = Color::Rgb(17, 17, 20);
pub const FG: Color = Color::Rgb(224, 224, 228);
pub const FG_DIM: Color = Color::Rgb(120, 120, 128);
pub const FG_FAINT: Color = Color::Rgb(72, 72, 80);
pub const BORDER: Color = Color::Rgb(56, 56, 62);
pub const BORDER_FOCUS: Color = Color::Rgb(210, 210, 216);
pub const ACCENT: Color = Color::Rgb(245, 245, 248);

pub const GOOD: Color = Color::Rgb(94, 201, 133);
pub const WARN: Color = Color::Rgb(224, 186, 88);
pub const BAD: Color = Color::Rgb(224, 96, 96);

pub fn severity_color(sev: Severity) -> Color {
    match sev {
        Severity::Good => GOOD,
        Severity::Warn => WARN,
        Severity::Bad => BAD,
        Severity::Neutral => FG_DIM,
    }
}

pub fn base() -> Style {
    Style::default().fg(FG).bg(BG)
}

pub fn panel_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(BORDER_FOCUS)
    } else {
        Style::default().fg(BORDER)
    }
}

pub fn title() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().fg(FG_DIM)
}

pub fn header_label() -> Style {
    Style::default().fg(FG_FAINT)
}

pub fn header_value() -> Style {
    Style::default().fg(FG).add_modifier(Modifier::BOLD)
}

pub fn table_header() -> Style {
    Style::default()
        .fg(BG)
        .bg(FG_DIM)
        .add_modifier(Modifier::BOLD)
}

pub fn selected_row() -> Style {
    Style::default()
        .fg(BG)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn key_hint() -> Style {
    Style::default().fg(BG).bg(FG_DIM).add_modifier(Modifier::BOLD)
}

pub fn command_bar() -> Style {
    Style::default().fg(ACCENT).bg(BG_PANEL)
}
