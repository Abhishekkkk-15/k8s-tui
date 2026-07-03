use ratatui::layout::{Constraint, Rect};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::data::ResourceKind;
use crate::theme;

pub fn draw(f: &mut Frame, app: &mut App, kind: ResourceKind, area: Rect) {
    let rows_data = app.visible_rows(kind);
    let columns = kind.columns();

    let header_cells = columns.iter().map(|c| Cell::from(*c));
    let header = Row::new(header_cells).style(theme::table_header()).height(1);

    let rows: Vec<Row> = rows_data
        .iter()
        .map(|r| {
            let cells = r.cells.iter().enumerate().map(|(i, text)| {
                let style = match r.status_col {
                    Some(c) if c == i => ratatui::style::Style::default().fg(theme::severity_color(r.severity)),
                    _ => ratatui::style::Style::default().fg(theme::FG),
                };
                Cell::from(text.clone()).style(style)
            });
            Row::new(cells)
        })
        .collect();

    let widths: Vec<Constraint> = columns.iter().map(|c| col_width(c)).collect();

    let title = if app.filter.is_empty() {
        format!(" {} ", kind.title())
    } else {
        format!(" {} (filter: /{}) ", kind.title(), app.filter)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::panel_border(true))
        .title(Span::styled(title, theme::title()));

    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(theme::selected_row())
        .highlight_symbol("➤ ")
        .column_spacing(1);

    if rows_data.is_empty() {
        app.table_state.select(None);
    } else if app.table_state.selected().is_none() {
        app.table_state.select(Some(0));
    }

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn col_width(name: &str) -> Constraint {
    match name {
        "NAMESPACE" => Constraint::Length(16),
        "NAME" | "OBJECT" => Constraint::Fill(2),
        "READY" | "STATUS" | "TYPE" | "RESTARTS" | "CPU" | "MEM" | "COUNT" | "AGE"
        | "DATA" | "DESIRED" | "CURRENT" | "UP-TO-DATE" | "AVAILABLE" | "ROLES" => {
            Constraint::Length(11)
        }
        "VERSION" => Constraint::Length(9),
        _ => Constraint::Fill(1),
    }
}
