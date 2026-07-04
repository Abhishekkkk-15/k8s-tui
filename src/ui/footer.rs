use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Mode, View};
use crate::data::ResourceKind;
use crate::theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let line0 = match app.mode {
        Mode::Command => Line::from(Span::styled(format!(":{}", app.input), theme::command_bar())),
        Mode::Filter => Line::from(Span::styled(format!("/{}", app.input), theme::command_bar())),
        Mode::CreateName => {
            let kind = app.create_kind().map(|k| k.title()).unwrap_or("resource");
            Line::from(Span::styled(
                format!("new {kind} name: {}", app.input),
                theme::command_bar(),
            ))
        }
        Mode::ConfirmDelete => {
            let name = app.pending_delete_name().unwrap_or("");
            Line::from(Span::styled(
                format!("delete '{name}'? y/n"),
                Style::default().fg(theme::BAD),
            ))
        }
        Mode::Edit => {
            let (kind, name) = app.edit_target_info().unwrap_or((ResourceKind::Pods, ""));
            Line::from(Span::styled(
                format!("edit {} '{}' {}: {}", kind.title(), name, kind.edit_field_label(), app.input),
                theme::command_bar(),
            ))
        }
        Mode::Normal => match &app.status_message {
            Some(msg) => Line::from(Span::styled(msg.clone(), Style::default().fg(theme::WARN))),
            None => Line::default(),
        },
    };
    f.render_widget(Paragraph::new(line0), chunks[0]);

    let hints = hints_for(app);
    let mut spans = Vec::with_capacity(hints.len() * 2);
    for (key, desc) in hints {
        spans.push(Span::styled(format!(" {} ", key), theme::key_hint()));
        spans.push(Span::styled(format!(" {} ", desc), theme::dim()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), chunks[1]);
}

fn hints_for(app: &App) -> Vec<(&'static str, &'static str)> {
    match app.current_view() {
        View::Clusters => vec![
            ("\u{2191}\u{2193}/jk", "move"),
            ("enter", "connect"),
            ("?", "help"),
            ("q", "quit"),
        ],
        View::Table(kind) => {
            let mut v = vec![("\u{2191}\u{2193}/jk", "move"), ("enter/d", "describe")];
            if kind == ResourceKind::Pods {
                v.push(("l", "logs"));
            }
            v.push((":", "command"));
            v.push(("/", "filter"));
            if kind.namespaced() {
                v.push(("n", "namespace"));
            }
            if kind.creatable() {
                v.push(("ctrl+n", "create"));
            }
            if kind.editable() {
                v.push(("e", "edit"));
            }
            v.push(("ctrl+d", "delete"));
            v.push(("[ ]", "kind"));
            v.push(("c", "clusters"));
            v.push(("esc", "back"));
            v.push(("?", "help"));
            v.push(("q", "quit"));
            v
        }
        View::Detail { .. } | View::Logs { .. } => {
            vec![("esc", "back"), ("c", "clusters"), ("?", "help"), ("q", "quit")]
        }
    }
}
