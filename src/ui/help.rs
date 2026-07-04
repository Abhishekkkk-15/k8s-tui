use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::theme;

const LINES: &[&str] = &[
    "Navigation",
    "  j/k, up/down    move selection",
    "  g / G           jump to top / bottom",
    "  enter, d        describe selected item",
    "  l               view pod logs (Pods view only)",
    "  esc             back / cancel / clear filter",
    "",
    "Browsing",
    "  :<name>         jump to a resource (po, deploy, rs, sts, svc,",
    "                  ing, no, ns, cm, secret, ev, pvc)",
    "  :ctx            back to cluster list",
    "  [ / ]           cycle through resource kinds",
    "  n               cycle namespace filter",
    "  /               filter visible rows",
    "  c               back to cluster list",
    "  r               force refresh",
    "",
    "Editing (mock data only)",
    "  ctrl+n          create a new resource of the current kind",
    "  ctrl+d          delete the selected resource (asks to confirm)",
    "",
    "General",
    "  ?               toggle this help",
    "  q, ctrl+c       quit",
];

pub fn draw(f: &mut Frame, area: Rect) {
    let popup = centered_rect(64, 70, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border(true))
        .title(Span::styled(" Help ", theme::title()));

    let lines: Vec<Line> = LINES
        .iter()
        .map(|l| Line::from(Span::styled(*l, theme::dim())))
        .collect();

    let p = Paragraph::new(lines).block(block).style(theme::base());
    f.render_widget(p, popup);
}

fn centered_rect(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vertical[1])[1]
}
