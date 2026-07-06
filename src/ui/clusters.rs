use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::App;
use crate::theme;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border(true))
        .title(Span::styled(" Clusters ", theme::title()));

    let items: Vec<ListItem> = app
        .backend
        .clusters()
        .iter()
        .map(|c| {
            let line = Line::from(vec![
                Span::styled(format!("{:<16}", c.name), theme::header_value()),
                Span::styled(format!("{:<10}", c.provider.label()), theme::dim()),
                Span::styled(format!("{:<22}", c.context), theme::dim()),
                Span::styled(format!("nodes:{:<3}", c.node_count), theme::dim()),
                Span::styled(format!("k8s:{}", c.k8s_version), theme::dim()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected_row())
        .highlight_symbol("➤ ");

    f.render_stateful_widget(list, area, &mut app.cluster_state);
}
