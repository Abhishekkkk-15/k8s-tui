use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::data::ResourceKind;
use crate::theme;

pub fn draw(
    f: &mut Frame,
    app: &App,
    kind: ResourceKind,
    namespace: Option<&str>,
    name: &str,
    area: Rect,
) {
    let text = app.backend.describe(kind, namespace, name);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border(true))
        .title(Span::styled(format!(" {}: {} ", kind.title(), name), theme::title()));

    let p = Paragraph::new(text)
        .block(block)
        .style(theme::base())
        .wrap(Wrap { trim: false });

    f.render_widget(p, area);
}
