use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn draw(f: &mut Frame, app: &App, namespace: &str, pod: &str, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border(true))
        .title(Span::styled(format!(" logs: {}/{} ", namespace, pod), theme::title()));

    let inner_height = area.height.saturating_sub(2) as usize;
    let total = app.log_lines.len();
    let scroll = total.saturating_sub(inner_height) as u16;

    let lines: Vec<Line> = app
        .log_lines
        .iter()
        .map(|l| Line::from(Span::styled(l.clone(), level_style(l))))
        .collect();

    let p = Paragraph::new(lines).block(block).scroll((scroll, 0));
    f.render_widget(p, area);
}

fn level_style(line: &str) -> Style {
    if line.contains("ERROR") {
        Style::default().fg(theme::BAD)
    } else if line.contains("WARN") {
        Style::default().fg(theme::WARN)
    } else if line.contains("DEBUG") {
        Style::default().fg(theme::FG_FAINT)
    } else {
        Style::default().fg(theme::FG)
    }
}
