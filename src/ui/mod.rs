mod clusters;
mod detail;
mod footer;
mod header;
mod help;
mod logs;
mod table;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::{App, View};

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);

    header::draw(f, app, chunks[0]);

    match app.current_view() {
        View::Clusters => clusters::draw(f, app, chunks[1]),
        View::Table(kind) => table::draw(f, app, kind, chunks[1]),
        View::Detail { kind, namespace, name } => {
            detail::draw(f, app, kind, namespace.as_deref(), &name, chunks[1])
        }
        View::Logs { namespace, pod } => logs::draw(f, app, &namespace, &pod, chunks[1]),
    }

    footer::draw(f, app, chunks[2]);

    if app.help_visible {
        help::draw(f, area);
    }
}
